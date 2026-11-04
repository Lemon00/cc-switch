//! cc-switch CLI（A 方案）
//!
//! 默认只暴露 codex / agy 两个应用，不改官方源码的 AppType、不做 Cargo feature
//! 编译剔除：其他应用的代码仍在库里，只是不在 CLI 里列出、不让切。
//! 这是刻意选的便宜方案，见仓库根目录的 CLI 设计文档。
//! P4 之后应用选择是配置化的：`<配置目录>/frontend-apps.json` 的 cli 一项
//! 决定 --app 接受哪些应用，缺文件时默认就是 codex / agy。
//!
//! 配置目录与 GUI 同源：启动时先按 `CC_SWITCH_CONFIG_DIR` → GUI 的 Tauri
//! Store（`app_paths.json`）→ 默认 `~/.cc-switch` 的顺序确定，见
//! `cc_switch_core::config::apply_startup_app_config_dir_override`。CLI 不再
//! 自己拼 `$HOME/.cc-switch`，否则同一台机器上两端会打开两份数据库。
//!
//! 代理钩子同样是注入的（`FrontendHooks::with_state`）：事件写 stderr、OAuth
//! 用 AppState 的 Codex manager、故障转移用 `Weak<AppState>`。用
//! `FrontendHooks::default()` 会让 failover/OAuth 静默失效。
//!
//! - codex：供应商切换走官方 ProviderService（live 写入引擎、字节保留），
//!   代理走官方 ProxyService（前台 serve），推理参数走 #8970 解耦逻辑。
//! - agy：上游明确不支持 switch（`agy.switch.unsupported`），卡片只作用于
//!   它打开的终端。所以本 CLI 不提供 agy switch，只提供 agy launch：
//!   官方登录卡 → `agy --model=<显示名>`；第三方 Key 卡 →
//!   `agy --gemini_dir=<私有目录>`，Key/地址只经子进程环境传入，不进命令文本。
//! - 用量记录：仅对 CLI 自己的代理进程生效，默认关。开关存在
//!   `<配置目录>/cli.json`（`{"usage_recording": false}`），
//!   `proxy serve` 启动前按它调用 `set_cli_usage_recording`，
//!   GUI 进程的默认值和行为不受影响。
//! - 会话模块：本 CLI 不提供会话历史、回放、会话扫描、用量同步命令。

use std::env;
use std::fs;
use std::path::PathBuf;
use std::process::{exit, Command};
use std::sync::{Arc, Weak};

use cc_switch_core::events::{EventSink, FrontendHooks, OAuthStateProvider};
use cc_switch_core::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use cc_switch_core::proxy::providers::copilot_auth::CopilotAuthManager;
use cc_switch_core::proxy::providers::xai_oauth_auth::XaiOAuthManager;
use cc_switch_core::{config, frontend_apps, AppState, AppType, Database, ProviderService};
use tokio::sync::RwLock;

const VERSION: &str =
    "cc-switch-cli 0.1.0 (A方案, 基于官方 cc-switch v4.0.25, 白名单默认 codex/agy)";

fn help() -> &'static str {
    "用法: cc-switch-cli [--app codex|agy] <命令> [参数]\n\
     \n\
     本 CLI 默认只支持 codex / agy；白名单见 <配置目录>/frontend-apps.json 的 cli 一项，改配置可换应用。\n\
     未改官方源码的 AppType，也未做编译剔除，其他应用只是不在 CLI 里出现。\n\
     \n\
     codex:\n\
     \x20 cc-switch-cli --app codex provider list\n\
     \x20 cc-switch-cli --app codex provider current\n\
     \x20 cc-switch-cli --app codex provider switch <id>   # 也可用 use <id>\n\
     \x20 cc-switch-cli --app codex provider reasoning <id>  # 查看推理参数\n\
     \x20 cc-switch-cli --app codex provider reasoning <id> --supports-effort true \\\n\
     \x20\x20\x20\x20 --supports-thinking false --thinking-param none --effort-param reasoning_effort\n\
     \x20\x20\x20\x20 # ^ #8970 组合：只透传 reasoning_effort，不注入 thinking\n\
     \x20 cc-switch-cli --app codex proxy show\n\
     \x20 cc-switch-cli --app codex proxy serve   # 前台代理，Ctrl-C 退出\n\
     \x20 cc-switch-cli --app codex proxy usage [on|off]  # 默认 off，仅对本 CLI 代理生效\n\
     \n\
     agy（Antigravity CLI，不支持 switch，见上文）:\n\
     \x20 cc-switch-cli --app agy provider list\n\
     \x20 cc-switch-cli --app agy provider current\n\
     \x20 cc-switch-cli --app agy launch <id>            # 只打印启动命令，不执行\n\
     \x20 cc-switch-cli --app agy launch <id> --exec     # 真正启动 agy（Key 经环境传入）\n\
     \x20 cc-switch-cli --app agy launch <id> --exec -- --model foo  # 透传额外 agy 参数\n\
     \n\
     通用:\n\
     \x20 cc-switch-cli version\n\
     \x20 cc-switch-cli config path\n"
}

fn err(msg: &str) -> ! {
    eprintln!("错误: {msg}");
    exit(1);
}

/// 配置目录与 GUI 同源：`open_state()` 里已按 GUI 的 Store / 环境变量
/// 决定了覆盖目录，这里读核心的生效值，别再自己拼 `$HOME/.cc-switch`。
fn app_dir() -> PathBuf {
    config::get_app_config_dir()
}

fn cli_config_path() -> PathBuf {
    app_dir().join("cli.json")
}

/// CLI 专属用量开关：默认 false（关）。文件不存在或解析失败时都按关处理，
/// 避免一次写坏配置就把后台代理变成“偷偷记用量”。
fn usage_recording_enabled() -> bool {
    let Ok(text) = fs::read_to_string(cli_config_path()) else {
        return false;
    };
    let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
        return false;
    };
    v.get("usage_recording")
        .and_then(|x| x.as_bool())
        .unwrap_or(false)
}

fn set_usage_recording(enabled: bool) {
    let text = format!(
        "{{\n  \"usage_recording\": {enabled},\n  \"note\": \"仅对 CLI 自己的代理进程生效，GUI 不受影响\"\n}}\n"
    );
    if let Err(e) = fs::write(cli_config_path(), text) {
        err(&format!("写 {} 失败: {e}", cli_config_path().display()));
    }
    println!(
        "用量记录: {}（仅对本 CLI 代理生效，下次 proxy serve 起效）",
        if enabled { "开" } else { "关" }
    );
}

fn parse_bool(s: &str) -> bool {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => true,
        "false" | "0" | "no" | "off" => false,
        other => err(&format!("布尔值只能是 true/false，得到: {other}")),
    }
}

/// CLI 代理状态事件的 sink：没有前端可推，写到 stderr（低频事件）。
///
/// 用量写入通知走单独的 [`QuietEventSink`]：它每条日志都会触发（200ms
/// 防抖），打到 stderr 会淹没 `proxy serve` 的输出。
struct StderrEventSink;

impl EventSink for StderrEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        eprintln!("[event] {event}: {payload}");
    }
}

/// 静默事件接收端（debug 级别）。
///
/// 用于两条没有界面可刷新的通知链路：
/// - `usage_events::init`：不注入时 `notify_log_recorded` 直接 return，代理
///   记账、会话同步、归档、手动改用量 4 条路径的通知等于静默丢弃；
/// - WebDAV/S3 自动同步 worker 的状态事件。
/// CLI 的 `proxy serve` 是前台进程，事件打 stderr 会淹没有用输出，所以走
/// log（CLI 目前没装 logger，等于丢弃；将来接上 logger 即生效）。
struct QuietEventSink;

impl EventSink for QuietEventSink {
    fn emit(&self, event: &str, _payload: serde_json::Value) {
        // 走 log 而不是 stderr：`proxy serve` 是前台进程，每条日志（200ms
        // 防抖）打一行会淹没有用输出。CLI 目前没装 logger，因此这条 debug
        // 事实上被丢弃；注入的意义是让通知链路不再依赖 GUI 的 setup，
        // 将来 CLI 接上 logger（或换成计数）就能立刻拿到。
        log::debug!("[usage-event] {event}");
    }
}

/// CLI 的 OAuth 来源：Codex 用 AppState 自带的 manager（OAuth 路由的供应商
/// 因此能正常拿凭据）；Copilot/xAI 没有登录流程，保持 None。
///
/// 存 `Weak`：hooks 挂在 `AppState::proxy_service` 上，这里持强引用会和
/// `failover_state` 一样绕出 Arc 环，进程退出前内存都放不掉。
struct CliOAuthStateProvider(Weak<AppState>);

impl OAuthStateProvider for CliOAuthStateProvider {
    fn codex_oauth(&self) -> Option<Arc<CodexOAuthManager>> {
        self.0
            .upgrade()
            .map(|state| state.codex_oauth_manager.clone())
    }

    fn copilot_oauth(&self) -> Option<Arc<RwLock<CopilotAuthManager>>> {
        None
    }

    fn xai_oauth(&self) -> Option<Arc<RwLock<XaiOAuthManager>>> {
        None
    }
}

/// 组装 CLI 的钩子：事件 + Codex OAuth + 故障转移（`Weak<AppState>`）。
fn cli_hooks(state: &Arc<AppState>) -> FrontendHooks {
    FrontendHooks::with_state(
        Arc::new(StderrEventSink),
        Arc::new(CliOAuthStateProvider(Arc::downgrade(state))),
        state,
    )
}

fn open_state() -> Arc<AppState> {
    let db = match Database::init() {
        Ok(db) => Arc::new(db),
        Err(e) => err(&format!("打开数据库失败: {e}")),
    };
    let state = Arc::new(AppState::new(db));
    // 不注入钩子时 ProxyService 用 FrontendHooks::default()：故障转移永不执行、
    // OAuth 路由直接报错、事件静默丢弃。CLI 要的是和 GUI 不同的最小实现，
    // 不是空实现。
    state.proxy_service.set_hooks(cli_hooks(&state));
    // 用量写入通知是另一套 OnceLock sink（不在 ProxyService.hooks 里），
    // 不单独注入就会整条链路静默。
    cc_switch_core::usage_events::init(Arc::new(QuietEventSink));
    state
}

fn parse_app(args: &[String]) -> (AppType, Vec<String>) {
    // P4：应用选择配置化。文件缺失/解析失败时 load_whitelist 仍回退默认，
    // 但显式写空的列表不再被悄悄替换（见 frontend_apps::load_whitelist）。
    let whitelist = frontend_apps::load_whitelist("cli");
    if whitelist.is_empty() {
        err(&format!(
            "CLI 应用白名单为空：{} 的 cli 一项里没有可识别的应用名",
            frontend_apps::whitelist_path().display()
        ));
    }
    // 默认应用取白名单第一项（默认配置下就是 codex）。
    let mut app = whitelist[0].clone();
    let mut rest: Vec<String> = Vec::new();
    let mut i = 0;
    while i < args.len() {
        if args[i] == "--" {
            // `--` 之后是透传给子命令的参数（如 `agy launch <id> -- --app foo`），
            // 不能再当 CLI 自己的选项扫描，否则用户的目标参数会被吃掉。
            rest.extend(args[i..].iter().cloned());
            break;
        }
        if args[i] == "--app" {
            let v = args
                .get(i + 1)
                .unwrap_or_else(|| err("--app 后面要跟应用名"));
            let parsed = frontend_apps::parse_app_name(v)
                .unwrap_or_else(|| err(&format!("不支持的应用名 {v}")));
            if !whitelist.contains(&parsed) {
                let allowed = whitelist
                    .iter()
                    .map(|a| a.as_str())
                    .collect::<Vec<_>>()
                    .join(", ");
                err(&format!(
                    "应用 {} 不在 CLI 白名单里（{} 的 cli 一项），当前允许：{allowed}",
                    parsed.as_str(),
                    frontend_apps::whitelist_path().display()
                ));
            }
            app = parsed;
            i += 2;
        } else {
            rest.push(args[i].clone());
            i += 1;
        }
    }
    (app, rest)
}

/// 限制 glibc 的 malloc arena 数量。多线程下每个 arena 各自保留堆页，请求结束
/// 后不归还，表现为常驻 RSS 不回落（实测 300 并发后匿名页 1.7MB → 7.8MB）。
///
/// `MALLOC_ARENA_MAX` 环境变量必须在进程启动前设置；在 main 里 `set_var` 已经
/// 太晚（加载器和构造函数可能已用过 malloc），所以用 `mallopt`。
/// 只对 Linux + glibc 有意义：musl 只有一个 arena，macOS/Windows 不用 glibc。
#[cfg(all(target_os = "linux", target_env = "gnu"))]
fn tune_allocator() {
    // 尊重用户显式设置，方便排查内存时临时调大。
    if env::var_os("MALLOC_ARENA_MAX").is_some() {
        return;
    }
    // SAFETY: mallopt 是 glibc 的线程安全接口，M_ARENA_MAX 接受正整数。
    unsafe {
        libc::mallopt(libc::M_ARENA_MAX, 1);
    }
}

#[cfg(not(all(target_os = "linux", target_env = "gnu")))]
fn tune_allocator() {}

fn main() {
    // 必须在任何线程/运行时创建之前调用。
    tune_allocator();
    // 实测 AMD 的 Azure 网关会拒绝 rustls 的 HTTP/2 请求（400 HTML），走 h1 正常。
    // 只在本 CLI 进程设置；GUI 不设，继续协商 h2（见 core 的 proxy/http_client.rs）。
    env::set_var("CC_SWITCH_PROXY_HTTP1", "1");
    // 最先做：白名单、cli.json、数据库都按配置目录定位，而 GUI 里设的自定义
    // 目录只写在 Tauri Store 里。这里不先应用，CLI 就会和 GUI 各开一份数据库。
    let _ = config::apply_startup_app_config_dir_override();

    let raw: Vec<String> = env::args().skip(1).collect();
    let (app, args) = parse_app(&raw);
    if args.is_empty() || args[0] == "--help" || args[0] == "-h" || args[0] == "help" {
        print!("{help}", help = help());
        return;
    }
    if args[0] == "version" {
        println!("{VERSION}");
        return;
    }
    if args[0] == "config" && args.get(1).map(|s| s.as_str()) == Some("path") {
        // 三个路径同源（都走核心的 get_app_config_dir），自定义配置目录时不会再
        // 出现「打印的是 A、实际读的是 B」。
        println!("配置目录: {}", app_dir().display());
        println!("CLI 开关: {}", cli_config_path().display());
        println!("应用白名单: {}", frontend_apps::whitelist_path().display());
        return;
    }

    let state = open_state();
    match args[0].as_str() {
        "provider" => cmd_provider(&state, app, &args[1..]),
        "use" => {
            if app != AppType::Codex {
                err("agy 不支持 switch：卡片只作用于它打开的终端，请用 `agy launch <id>`");
            }
            let id = args.get(1).unwrap_or_else(|| err("用法: use <id>"));
            cmd_switch(&state, app, id);
        }
        "agy" | "launch" => {
            // 允许两种写法：`agy launch <id>` 和顶层 `launch <id>`（仅 agy）
            let id = if args[0] == "agy" {
                if args.get(1).map(|s| s.as_str()) != Some("launch") {
                    err("agy 只支持 launch：用法 `cc-switch-cli --app agy launch <id> [--exec]`");
                }
                args.get(2)
                    .unwrap_or_else(|| err("用法: agy launch <id> [--exec]"))
            } else {
                args.get(1)
                    .unwrap_or_else(|| err("用法: launch <id> [--exec]"))
            };
            if app != AppType::Agy {
                err("launch 只适用于 agy；codex 请用 provider switch");
            }
            cmd_agy_launch(&state, id, &args);
        }
        "proxy" => cmd_proxy(&state, app, &args[1..]),
        other => err(&format!("未知命令: {other}\n{}", help())),
    }
}

fn cmd_provider(state: &AppState, app: AppType, args: &[String]) {
    match args.first().map(|s| s.as_str()) {
        Some("list") => {
            let list = match ProviderService::list(state, app.clone()) {
                Ok(v) => v,
                Err(e) => err(&format!("列供应商失败: {e}")),
            };
            let current = ProviderService::current(state, app.clone()).unwrap_or_default();
            if list.is_empty() {
                println!("（{} 暂无供应商）", app.as_str());
                return;
            }
            for (id, p) in &list {
                let mark = if *id == current { " *" } else { "" };
                println!("{id}{mark}\t{}", p.name);
            }
        }
        Some("current") => {
            let cur = match ProviderService::current(state, app.clone()) {
                Ok(v) => v,
                Err(e) => err(&format!("取当前供应商失败: {e}")),
            };
            if cur.is_empty() {
                println!(
                    "（{} 暂无当前供应商；agy 本来就没有全局当前概念，见 launch）",
                    app.as_str()
                );
            } else {
                println!("{cur}");
            }
        }
        Some("switch") => {
            if app != AppType::Codex {
                err("agy 不支持 switch：卡片只作用于它打开的终端，请用 `agy launch <id>`");
            }
            let id = args
                .get(1)
                .unwrap_or_else(|| err("用法: provider switch <id>"));
            cmd_switch(state, app, id);
        }
        Some("reasoning") => {
            if app != AppType::Codex {
                err("reasoning 只适用于 codex");
            }
            let id = args
                .get(1)
                .unwrap_or_else(|| err("用法: provider reasoning <id> [选项]"));
            cmd_reasoning(state, id, &args[2..]);
        }
        _ => err("provider 只支持 list / current / switch / reasoning"),
    }
}

fn cmd_switch(state: &AppState, app: AppType, id: &str) {
    match ProviderService::switch(state, app, id) {
        Ok(result) => {
            println!("已切到 {id}");
            for w in &result.warnings {
                println!("警告: {w}");
            }
        }
        Err(e) => err(&format!("切换失败: {e}")),
    }
}

fn cmd_reasoning(state: &AppState, id: &str, opts: &[String]) {
    let mut provider = match state.db.get_provider_by_id(id, "codex") {
        Ok(Some(p)) => p,
        Ok(None) => err(&format!("codex 供应商 {id} 不存在")),
        Err(e) => err(&format!("读供应商失败: {e}")),
    };
    // 用 JSON 改 meta，避免 CLI 依赖未公开的 CodexChatReasoningConfig 类型名。
    let mut meta = serde_json::to_value(provider.meta.clone().unwrap_or_default())
        .unwrap_or(serde_json::Value::Null);
    if meta.is_null() {
        meta = serde_json::json!({});
    }
    let mut reasoning = meta
        .get("codexChatReasoning")
        .cloned()
        .unwrap_or(serde_json::json!({}));
    if opts.is_empty() {
        println!(
            "{}",
            serde_json::to_string_pretty(&reasoning).unwrap_or_default()
        );
        return;
    }
    // 解析 --key value
    let mut i = 0;
    let mut set_supports_thinking: Option<bool> = None;
    let mut set_supports_effort: Option<bool> = None;
    let mut set_thinking_param: Option<String> = None;
    let mut set_effort_param: Option<String> = None;
    let mut set_effort_value_mode: Option<String> = None;
    while i < opts.len() {
        let (k, v) = match (opts.get(i), opts.get(i + 1)) {
            (Some(k), Some(v)) => (k.as_str(), v.clone()),
            _ => err("reasoning 选项要成对出现，例如 --thinking-param none"),
        };
        match k {
            "--supports-thinking" => set_supports_thinking = Some(parse_bool(&v)),
            "--supports-effort" => set_supports_effort = Some(parse_bool(&v)),
            "--thinking-param" => {
                if !["thinking", "enable_thinking", "reasoning_split", "none"].contains(&v.as_str())
                {
                    err(
                        "thinking-param 只能是 thinking / enable_thinking / reasoning_split / none",
                    );
                }
                set_thinking_param = Some(v);
            }
            "--effort-param" => {
                if !["reasoning_effort", "reasoning.effort", "none"].contains(&v.as_str()) {
                    err("effort-param 只能是 reasoning_effort / reasoning.effort / none");
                }
                set_effort_param = Some(v);
            }
            "--effort-value-mode" => set_effort_value_mode = Some(v),
            _ => err(&format!("未知 reasoning 选项: {k}")),
        }
        i += 2;
    }
    // #8970 解耦：绝不由 effort 派生 thinking；thinking-param 显式值优先。
    if let Some(v) = set_supports_thinking {
        reasoning["supportsThinking"] = serde_json::Value::Bool(v);
    }
    if let Some(v) = set_supports_effort {
        reasoning["supportsEffort"] = serde_json::Value::Bool(v);
    }
    if let Some(v) = set_thinking_param {
        reasoning["thinkingParam"] = serde_json::Value::String(v);
    } else if set_supports_thinking == Some(false) {
        // 思考模式关、又没显式给 param 时，默认 none（不注入），而不是 thinking。
        reasoning["thinkingParam"] = serde_json::Value::String("none".to_string());
    } else if set_supports_thinking == Some(true) && reasoning.get("thinkingParam").is_none() {
        reasoning["thinkingParam"] = serde_json::Value::String("thinking".to_string());
    }
    if let Some(v) = set_effort_param {
        reasoning["effortParam"] = serde_json::Value::String(v);
    }
    if let Some(v) = set_effort_value_mode {
        reasoning["effortValueMode"] = serde_json::Value::String(v);
    }
    meta["codexChatReasoning"] = reasoning.clone();
    // 先反序列化再备份：转换失败时数据库还没动，留着原 meta 直接退出，
    // 不会出现「备份了、然后写进去一个空 meta」。
    let parsed_meta = match serde_json::from_value(meta) {
        Ok(parsed) => parsed,
        Err(e) => err(&format!(
            "推理参数无法写回供应商 meta（原配置未修改、数据库未写入）: {e}"
        )),
    };
    // 备份后再写（改 meta 只影响代理转换，不写 live 文件）。
    // 用 core 的一致性快照备份（sqlite backup API，走 backups/ 目录、带清理），
    // 不再自己 fs::copy 主库：那会漏掉 WAL 里未 checkpoint 的提交，
    // 失败时也只是打一行警告就当成功。
    match state.db.backup_database_file() {
        Ok(Some(bak)) => println!("已备份数据库到 {}", bak.display()),
        Ok(None) => {}
        Err(e) => eprintln!("警告: 数据库备份失败（继续写入）: {e}"),
    }
    provider.meta = Some(parsed_meta);
    if let Err(e) = state.db.save_provider("codex", &provider) {
        err(&format!("保存推理参数失败: {e}"));
    }
    println!(
        "{}",
        serde_json::to_string_pretty(&reasoning).unwrap_or_default()
    );
    println!("注意：推理参数只影响代理转换（路由模式），不写 codex 的 live 文件。");
}

fn safe_profile_dir_name(provider_id: &str) -> String {
    // 与上游 agy_config::profile_dir_name 同规则，保持跨版本稳定。
    let safe = !provider_id.is_empty()
        && provider_id != "."
        && provider_id != ".."
        && provider_id.len() <= 64
        && provider_id
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'));
    if safe {
        return provider_id.to_string();
    }
    let prefix: String = provider_id
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
        .take(32)
        .collect();
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in provider_id.as_bytes() {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    format!("{prefix}-{h:016x}")
}

fn cmd_agy_launch(state: &AppState, id: &str, args: &[String]) {
    let provider = match state.db.get_provider_by_id(id, "agy") {
        Ok(Some(p)) => p,
        Ok(None) => err(&format!("agy 供应商 {id} 不存在")),
        Err(e) => err(&format!("读供应商失败: {e}")),
    };
    let settings = &provider.settings_config;
    let env_obj = settings.get("env");
    let key = env_obj
        .and_then(|e| e.get("GEMINI_API_KEY"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let base_url = env_obj
        .and_then(|e| e.get("GOOGLE_GEMINI_BASE_URL"))
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim()
        .to_string();
    let model = settings
        .pointer("/config/model/name")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .or_else(|| {
            env_obj
                .and_then(|e| e.get("GEMINI_MODEL"))
                .and_then(|v| v.as_str())
                .map(|s| s.to_string())
        });
    let exec = args.iter().any(|a| a == "--exec");
    let extra: Vec<String> = match args.iter().position(|a| a == "--") {
        Some(pos) => args[pos + 1..].to_vec(),
        None => Vec::new(),
    };
    if !key.is_empty() {
        if base_url.is_empty() {
            err("该 agy 供应商有 Key 但没有 GOOGLE_GEMINI_BASE_URL，拒绝启动（上游同规则）");
        }
        // Key 模式：建私有目录，写 modelProvider/model，不写真实 settings.json。
        let dir = app_dir().join("agy").join(safe_profile_dir_name(id));
        let settings_path = dir.join("antigravity-cli").join("settings.json");
        let mut existing = fs::read_to_string(&settings_path)
            .ok()
            .and_then(|t| serde_json::from_str::<serde_json::Value>(&t).ok())
            .unwrap_or(serde_json::json!({}));
        existing["modelProvider"] = serde_json::Value::String("gemini".to_string());
        match &model {
            Some(m) => existing["model"] = serde_json::Value::String(m.clone()),
            None => {
                if let Some(obj) = existing.as_object_mut() {
                    obj.remove("model");
                }
            }
        }
        if let Some(parent) = settings_path.parent() {
            if let Err(e) = fs::create_dir_all(parent) {
                err(&format!("建私有目录失败: {e}"));
            }
        }
        if let Err(e) = fs::write(
            &settings_path,
            serde_json::to_string_pretty(&existing).unwrap(),
        ) {
            err(&format!("写私有 settings.json 失败: {e}"));
        }
        // <私有目录>/config 软链回 ~/.gemini/config；已是真实目录时不动它。
        let home = env::var("HOME").unwrap_or_else(|_| ".".to_string());
        let shared = PathBuf::from(home).join(".gemini").join("config");
        let _ = fs::create_dir_all(&shared);
        let link = dir.join("config");
        match fs::symlink_metadata(&link) {
            Ok(m) if m.file_type().is_symlink() => {
                if fs::read_link(&link).ok().as_deref() != Some(shared.as_path()) {
                    let _ = fs::remove_file(&link);
                    #[cfg(unix)]
                    let _ = std::os::unix::fs::symlink(&shared, &link);
                }
            }
            Ok(_) => {
                println!("注意：{} 已是真实目录，按上游规则不动它", link.display());
            }
            Err(_) => {
                #[cfg(unix)]
                let _ = std::os::unix::fs::symlink(&shared, &link);
            }
        }
        let mut cmd = vec!["agy".to_string(), format!("--gemini_dir={}", dir.display())];
        cmd.extend(extra.clone());
        println!("启动命令（Key 不进命令文本，只经子进程环境传入）:");
        println!("  {}", cmd.join(" "));
        println!("  环境: GEMINI_API_KEY=<已设置，不打印> GOOGLE_GEMINI_BASE_URL={base_url}");
        if exec {
            let status = Command::new("agy")
                .arg(format!("--gemini_dir={}", dir.display()))
                .args(&extra)
                .env("GEMINI_API_KEY", &key)
                .env("GOOGLE_GEMINI_BASE_URL", &base_url)
                .status();
            match status {
                Ok(s) => exit(s.code().unwrap_or(1)),
                Err(e) => err(&format!("启动 agy 失败（是否已装 agy？）: {e}")),
            }
        }
    } else {
        let mut cmd = vec!["agy".to_string()];
        if let Some(m) = &model {
            cmd.push(format!("--model={m}"));
        }
        cmd.extend(extra.clone());
        println!("启动命令（官方登录卡，直接跑原生 agy，模型只对这次会话生效）:");
        println!("  {}", cmd.join(" "));
        if exec {
            let mut c = Command::new("agy");
            if let Some(m) = &model {
                c.arg(format!("--model={m}"));
            }
            c.args(&extra);
            match c.status() {
                Ok(s) => exit(s.code().unwrap_or(1)),
                Err(e) => err(&format!("启动 agy 失败（是否已装 agy？）: {e}")),
            }
        }
    }
}

fn cmd_proxy(state: &AppState, app: AppType, args: &[String]) {
    match args.first().map(|s| s.as_str()) {
        Some("show") => {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build();
            match rt {
                Ok(rt) => match rt.block_on(state.proxy_service.get_config()) {
                    Ok(cfg) => {
                        println!("代理配置（数据库当前值）:");
                        println!("  监听: {}:{}", cfg.listen_address, cfg.listen_port);
                        println!(
                            "  应用: {}（本 CLI 只做 codex 代理前台 serve；agy 不走代理）",
                            app.as_str()
                        );
                        println!(
                            "  用量记录（CLI 专属）: {}",
                            if usage_recording_enabled() {
                                "开"
                            } else {
                                "关（默认）"
                            }
                        );
                    }
                    Err(e) => err(&format!("读代理配置失败: {e}")),
                },
                Err(e) => err(&format!("建运行时失败: {e}")),
            }
        }
        Some("usage") => match args.get(1).map(|s| s.as_str()) {
            Some("on") => set_usage_recording(true),
            Some("off") => set_usage_recording(false),
            _ => println!(
                "用量记录（CLI 专属）: {}",
                if usage_recording_enabled() {
                    "开"
                } else {
                    "关（默认）"
                }
            ),
        },
        Some("serve") => {
            if app != AppType::Codex {
                err("proxy serve 只适用于 codex；agy 不走代理，请用 agy launch");
            }
            let enabled = usage_recording_enabled();
            cc_switch_core::set_cli_usage_recording(enabled);
            // serve 是长驻进程：和 GUI 一样启动 WebDAV/S3 自动同步 worker
            //（远端没配置时 worker 自己空转）。一次性命令不启，短进程没必要
            // 拉起后台 runtime。
            cc_switch_core::services::webdav_auto_sync::start_worker(
                state.db.clone(),
                Arc::new(QuietEventSink),
            );
            cc_switch_core::services::s3_auto_sync::start_worker(
                state.db.clone(),
                Arc::new(QuietEventSink),
            );
            println!(
                "启动 codex 代理（前台，Ctrl-C 退出；用量记录: {}，仅对本进程生效）",
                if enabled { "开" } else { "关" }
            );
            let rt = match tokio::runtime::Builder::new_multi_thread()
                // 代理是 IO 密集型：默认按 CPU 核数起 worker（本机 6 核），
                // 每个线程都会推高 glibc arena/栈占用。2 个足够，和 core 的
                // background runtime 保持一致。
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(rt) => rt,
                Err(e) => err(&format!("建运行时失败: {e}")),
            };
            let info = match rt.block_on(state.proxy_service.start()) {
                Ok(info) => info,
                Err(e) => err(&format!("启动代理失败: {e}")),
            };
            println!(
                "代理已起: {}:{}（客户端请把上游地址指到这里，或用 GUI 的路由接管）",
                info.address, info.port
            );
            // 前台等待 Ctrl-C；收到后停代理再退出。
            let _ = rt.block_on(async {
                let _ = tokio::signal::ctrl_c().await;
            });
            let _ = rt.block_on(state.proxy_service.stop());
            println!("代理已停");
        }
        _ => err("proxy 只支持 show / serve / usage [on|off]"),
    }
}
