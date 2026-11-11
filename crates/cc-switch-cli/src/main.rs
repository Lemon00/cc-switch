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
//!   代理走官方 ProxyService（`proxy enable` 拉起后台进程、`proxy disable` 停止），
//!   推理参数走 #8970 解耦逻辑。
//! - agy：上游明确不支持 switch（`agy.switch.unsupported`），卡片只作用于
//!   它打开的终端。所以本 CLI 不提供 agy switch，只提供 agy launch：
//!   官方登录卡 → `agy --model=<显示名>`；第三方 Key 卡 →
//!   `agy --gemini_dir=<私有目录>`，Key/地址只经子进程环境传入，不进命令文本。
//! - 用量记录：仅对 CLI 自己的代理进程生效，默认关。开关存在
//!   `<配置目录>/cli.json`（`{"usage_recording": false}`），
//!   `proxy enable` 拉起的后台进程启动前按它调用 `set_cli_usage_recording`，
//!   GUI 进程的默认值和行为不受影响。
//! - 会话模块：本 CLI 不提供会话历史、回放、会话扫描、用量同步命令。

use std::env;
use std::fs::{self, OpenOptions};
use std::net::{TcpStream, ToSocketAddrs};
use std::path::{Path, PathBuf};
use std::process::{exit, Command, Stdio};
use std::sync::{Arc, Weak};
use std::thread;
use std::time::{Duration, Instant};

use cc_switch_core::events::{EventSink, FrontendHooks, OAuthStateProvider};
use cc_switch_core::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use cc_switch_core::proxy::providers::copilot_auth::CopilotAuthManager;
use cc_switch_core::proxy::providers::xai_oauth_auth::XaiOAuthManager;
use cc_switch_core::{config, frontend_apps, AppState, AppType, Database, ProviderService};
use tokio::sync::RwLock;

const VERSION: &str = concat!(
    "cc-switch-cli ",
    env!("CARGO_PKG_VERSION"),
    " (A方案, 基于官方 cc-switch v",
    env!("CARGO_PKG_VERSION"),
    ", 白名单默认 codex/agy)"
);

/// `proxy enable` 拉起的后台进程用的内部命令。不出现在 help 里，也不做参数校验包装：
/// 它始终由本 CLI 自己用同一个可执行文件 + 同一个 `--app` 拉起。
const PROXY_DAEMON_CMD: &str = "__proxy-daemon";

/// 逐行拼，不用 `\` 续行：源码是 CRLF，`\n\` 混着行尾的 `\r` 会把多出来的回车打进
/// 输出（help 里因此多过一行空白、缩进也对不齐）。
fn help() -> String {
    [
        "用法: cc-switch-cli [--app codex|agy] <命令> [参数]",
        "",
        "本 CLI 默认只支持 codex / agy；白名单见 <配置目录>/frontend-apps.json 的 cli 一项，改配置可换应用。",
        "未改官方源码的 AppType，也未做编译剔除，其他应用只是不在 CLI 里出现。",
        "",
        "codex:",
        "  cc-switch-cli --app codex provider list",
        "  cc-switch-cli --app codex provider current",
        "  cc-switch-cli --app codex provider switch <id>   # 也可用 use <id>",
        "    # 路由 / 聚合模式下会先确保后台代理在跑，再重写客户端配置（含模型目录）；--no-attach 可关掉",
        "  cc-switch-cli --app codex provider reasoning <id>  # 查看推理参数",
        "  cc-switch-cli --app codex provider reasoning <id> --supports-effort true \\",
        "    --supports-thinking false --thinking-param none --effort-param reasoning_effort",
        "    # ^ #8970 组合：只透传 reasoning_effort，不注入 thinking",
        "    # 其余可选项：--effort-value-mode passthrough|low_high|deepseek|openrouter|zen",
        "    #             --output-format auto|reasoning_content|reasoning|reasoning_details|think_tags",
        "  cc-switch-cli --app codex proxy show",
        "  cc-switch-cli --app codex proxy enable    # 启动后台代理，并按当前路由重写客户端配置（含模型目录）",
        "  cc-switch-cli --app codex proxy disable   # 停止后台代理，并把客户端配置指回直连",
        "  cc-switch-cli --app codex proxy usage [on|off]  # 默认 off，仅对本 CLI 代理生效",
        "  cc-switch-cli --app codex proxy log [on|off]    # 默认 off，关闭时不写 cli-proxy.log",
        "",
        "agy（Antigravity CLI，不支持 switch，见上文）:",
        "  cc-switch-cli --app agy provider list",
        "  cc-switch-cli --app agy provider current",
        "  cc-switch-cli --app agy launch <id>            # 只打印启动命令，不执行",
        "  cc-switch-cli --app agy launch <id> --exec     # 真正启动 agy（Key 经环境传入）",
        "  cc-switch-cli --app agy launch <id> --exec -- --model foo  # 透传额外 agy 参数",
        "",
        "通用:",
        "  cc-switch-cli version / --version / -V",
        "  cc-switch-cli config path",
    ]
    .join("\n")
        + "\n"
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

fn proxy_pid_path() -> PathBuf {
    app_dir().join("cli-proxy.pid")
}

fn proxy_log_path() -> PathBuf {
    app_dir().join("cli-proxy.log")
}

fn read_proxy_pid() -> Option<u32> {
    fs::read_to_string(proxy_pid_path())
        .ok()?
        .trim()
        .parse::<u32>()
        .ok()
        .filter(|pid| *pid > 1)
}

fn write_proxy_pid(pid: u32) {
    if let Err(e) = fs::write(proxy_pid_path(), pid.to_string()) {
        eprintln!("警告: 写 pid 文件失败: {e}");
    }
}

fn remove_proxy_pid() {
    let _ = fs::remove_file(proxy_pid_path());
}

#[cfg(unix)]
fn pid_alive(pid: u32) -> bool {
    // SAFETY: kill(pid, 0) 只查询进程是否存在，不真的发信号。
    unsafe { libc::kill(pid as libc::pid_t, 0) == 0 }
}

/// 取 pid 对应进程的映像名，顺便当存活判断用。
///
/// 用 CSV 输出精确比对 PID 列：默认输出的内存列（`12,345 K`）里也可能出现 pid 的数字，
/// 用 `contains` 会误判存活。
#[cfg(windows)]
fn windows_image_name(pid: u32) -> Option<String> {
    let out = Command::new("tasklist")
        .args(["/FI", &format!("PID eq {pid}"), "/NH", "/FO", "CSV"])
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = String::from_utf8_lossy(&out.stdout);
    for line in text.lines() {
        let cols: Vec<&str> = line.split("\",\"").map(|c| c.trim_matches('"')).collect();
        if cols.len() >= 2 && cols[1].trim() == pid.to_string() {
            return Some(cols[0].trim().to_string());
        }
    }
    None
}

#[cfg(windows)]
fn pid_alive(pid: u32) -> bool {
    windows_image_name(pid).is_some()
}

/// PID 可能被系统复用：停进程前再确认一次 cmdline 是自己的守护进程，
/// 免得 `disable` 误杀一个无关进程。
#[cfg(unix)]
fn pid_is_our_daemon(pid: u32) -> bool {
    fs::read_to_string(format!("/proc/{pid}/cmdline"))
        .map(|cmd| cmd.contains(PROXY_DAEMON_CMD))
        .unwrap_or(false)
}

#[cfg(windows)]
fn pid_is_our_daemon(pid: u32) -> bool {
    // PID 会被系统复用：能读到映像名时确认是自己的可执行文件，免得 `proxy disable`
    // 用 taskkill /T /F 杀掉无关进程树；读不到（tasklist 不可用）时退回原来的宽松判断。
    let Some(name) = windows_image_name(pid) else {
        return true;
    };
    match env::current_exe().ok().and_then(|exe| {
        exe.file_name()
            .map(|name| name.to_string_lossy().to_string())
    }) {
        Some(exe) => exe.eq_ignore_ascii_case(&name),
        None => true,
    }
}

fn running_proxy_pid() -> Option<u32> {
    let pid = read_proxy_pid()?;
    (pid_alive(pid) && pid_is_our_daemon(pid)).then_some(pid)
}

/// 把同一个可执行文件拉成独立于终端的后台进程。开启 proxy_log 时 stdout/stderr
/// 落到日志文件，否则直接丢弃（默认不写 cli-proxy.log），避免终端关闭后
/// 守护进程的打印变成 SIGPIPE/EPIPE。
fn spawn_proxy_daemon(app: &AppType, exe: &Path, log: Option<&fs::File>) -> Result<u32, String> {
    let mut cmd = Command::new(exe);
    cmd.args(["--app", app.as_str(), "proxy", PROXY_DAEMON_CMD])
        .stdin(Stdio::null());
    match log {
        Some(file) => {
            let stdout = file
                .try_clone()
                .map_err(|e| format!("复制日志句柄失败: {e}"))?;
            let stderr = file
                .try_clone()
                .map_err(|e| format!("复制日志句柄失败: {e}"))?;
            cmd.stdout(Stdio::from(stdout)).stderr(Stdio::from(stderr));
        }
        None => {
            cmd.stdout(Stdio::null()).stderr(Stdio::null());
        }
    }

    #[cfg(unix)]
    {
        use std::os::unix::process::CommandExt;
        // 脱离控制终端：enable 返回后 shell 退出、SIGHUP 不会带走代理。
        // SAFETY: pre_exec 的闭包只在 fork 出来的子进程、exec 之前运行，里面只
        // 调用 async-signal-safe 的 setsid，不碰 Rust 的分配器/锁。
        unsafe {
            cmd.pre_exec(|| {
                libc::setsid();
                Ok(())
            });
        }
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        const DETACHED_PROCESS: u32 = 0x0000_0008;
        cmd.creation_flags(DETACHED_PROCESS);
    }

    cmd.spawn()
        .map(|child| child.id())
        .map_err(|e| format!("拉起后台代理失败: {e}"))
}

/// 守护进程等 Ctrl-C（手动跑内部命令时）或 SIGTERM（`proxy disable` 发的）。
#[cfg(unix)]
async fn wait_for_proxy_shutdown() {
    use tokio::signal::unix::{signal, SignalKind};
    let mut term = match signal(SignalKind::terminate()) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("警告: 注册 SIGTERM 处理失败，只能 Ctrl-C 退出: {e}");
            let _ = tokio::signal::ctrl_c().await;
            return;
        }
    };
    tokio::select! {
        _ = tokio::signal::ctrl_c() => {}
        _ = term.recv() => {}
    }
}

#[cfg(not(unix))]
async fn wait_for_proxy_shutdown() {
    let _ = tokio::signal::ctrl_c().await;
}

/// 拉起后台代理并等它就绪，返回 pid。`logging` 为真时日志写 `cli-proxy.log`，并等代理真正
/// bind 成功（日志里的「代理已起」）再返回；否则只看守护进程是否还活着。
///
/// `proxy enable` 和「路由 / 聚合模式下切换供应商之后接上客户端」共用这一条路径：客户端文件
/// 一旦被接上，代理就必须在跑，否则 Codex 的每个请求都会打到没人监听的端口。
fn start_proxy_daemon(app: &AppType, logging: bool) -> Result<u32, String> {
    // 到这里 pid 文件要么不存在，要么是指向已退出/无关进程的陈旧文件。
    remove_proxy_pid();

    let log_path = proxy_log_path();
    // 默认不写 cli-proxy.log；只有显式 `proxy log on` 才创建/截断日志文件。
    let log = if logging {
        Some(
            OpenOptions::new()
                .create(true)
                .write(true)
                .truncate(true)
                .open(&log_path)
                .map_err(|e| format!("打开日志 {} 失败: {e}", log_path.display()))?,
        )
    } else {
        None
    };
    let exe = env::current_exe().map_err(|e| format!("取当前可执行文件路径失败: {e}"))?;
    let pid = spawn_proxy_daemon(app, &exe, log.as_ref())?;
    write_proxy_pid(pid);

    if !logging {
        // 日志关闭时没有「代理已起」可读：靠守护进程存活判断就绪，
        // 启动失败（bind 失败等）会让它立刻 err() 退出。
        let ready_at = Instant::now() + Duration::from_secs(2);
        while Instant::now() < ready_at {
            if !pid_alive(pid) {
                remove_proxy_pid();
                return Err(concat!(
                    "后台代理启动失败（进程已退出）：",
                    "GUI 正在运行时它自带的代理会占着同一端口，这时不用再 enable；",
                    "否则运行 `proxy log on` 打开日志后重新 enable 可查看原因"
                )
                .to_string());
            }
            thread::sleep(Duration::from_millis(150));
        }
        return Ok(pid);
    }

    // 开启日志：等代理真正 bind 成功再返回；失败时把日志尾巴打出来。
    // 日志里的「代理已起」由守护进程在 start() 成功后打印。
    let deadline = Instant::now() + Duration::from_secs(15);
    loop {
        if let Ok(text) = fs::read_to_string(&log_path) {
            if text.contains("代理已起:") {
                return Ok(pid);
            }
            if text.contains("启动代理失败") {
                let _ = fs::remove_file(proxy_pid_path());
                return Err(format!("后台代理启动失败，详见 {}", log_path.display()));
            }
        }
        if !pid_alive(pid) {
            remove_proxy_pid();
            return Err(format!("后台代理进程已退出，详见 {}", log_path.display()));
        }
        if Instant::now() >= deadline {
            remove_proxy_pid();
            return Err(format!("等待后台代理就绪超时，详见 {}", log_path.display()));
        }
        thread::sleep(Duration::from_millis(150));
    }
}

/// enable / disable：进程管理 + 客户端接上/分离。
///
/// 守护进程只管代理本身；客户端文件（`config.toml` / 模型目录）由发起命令的父进程
/// 在代理就绪后接上、在代理停掉后分离。这样常驻进程不必写用户配置，也避免父进程
/// 和守护进程同时 attach。开库放在起/停之后，是 best-effort：失败只警告，不改命令结果。
fn cmd_proxy_control(app: AppType, action: &str) {
    if action == "enable" {
        if app != AppType::Codex {
            err("proxy enable 只适用于 codex；agy 不走代理，请用 agy launch");
        }
        if let Some(pid) = running_proxy_pid() {
            println!("后台代理已在运行（pid {pid}），无需重复 enable");
        } else {
            let logging = proxy_log_enabled();
            let pid = match start_proxy_daemon(&app, logging) {
                Ok(pid) => pid,
                Err(e) => err(&e),
            };
            println!("后台代理已启动（pid {pid}）");
            if logging {
                println!("日志: {}", proxy_log_path().display());
            } else {
                println!("日志: 已关闭（proxy log on 可打开）");
            }
            println!("停止: cc-switch-cli --app {} proxy disable", app.as_str());
        }
        // GUI 退出时 detach_all 把客户端写回了直连，只起代理不会重写 config.toml
        // 和模型目录，Codex 的模型列表会停在旧的那几个。代理就绪后补一次接上。
        match try_open_state() {
            Ok(state) => ensure_client_live(&state, &app),
            Err(e) => println!("警告: 客户端配置保持原样（{e}）"),
        }
        return;
    }

    // disable：即使没在跑也当成功，方便脚本里直接调用。
    let Some(pid) = read_proxy_pid() else {
        println!("后台代理未在运行");
        return;
    };
    if !pid_alive(pid) || !pid_is_our_daemon(pid) {
        remove_proxy_pid();
        println!("后台代理未在运行（已清理陈旧的 pid 文件）");
        return;
    }

    #[cfg(unix)]
    {
        // SAFETY: pid 已确认是本 CLI 的守护进程；SIGTERM 触发它优雅停代理后退出。
        unsafe {
            libc::kill(pid as libc::pid_t, libc::SIGTERM);
        }
        let deadline = Instant::now() + Duration::from_secs(5);
        while pid_alive(pid) && Instant::now() < deadline {
            thread::sleep(Duration::from_millis(100));
        }
        if pid_alive(pid) {
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
        }
    }
    #[cfg(windows)]
    {
        let status = Command::new("taskkill")
            .args(["/PID", &pid.to_string(), "/T", "/F"])
            .status();
        match status {
            Ok(s) if s.success() => {}
            Ok(s) => err(&format!("停止后台代理失败: taskkill 退出码 {s}")),
            Err(e) => err(&format!("执行 taskkill 失败: {e}")),
        }
    }
    remove_proxy_pid();
    println!("后台代理已停止（pid {pid}）");
    // 停完代理要把客户端指回直连，否则 config.toml 还指向没人监听的端口。
    // detach_all 只分离已接上的应用；已经是分离态时 exit_locked 直接返回，不会误写。
    // 只在确实停掉自己的守护进程时做，pid 文件缺失/过期那几条出口不碰 GUI 管的客户端。
    match try_open_state() {
        Ok(state) => {
            match tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
            {
                Ok(rt) => {
                    rt.block_on(cc_switch_core::mode::controller::detach_all(&state));
                    println!("客户端配置已分离（指回直连）");
                }
                Err(e) => println!("警告: 客户端配置保持接上（建运行时失败: {e}）"),
            }
        }
        Err(e) => println!("警告: 客户端配置保持接上（{e}）"),
    }
}

/// CLI 专属开关都放在 cli.json，默认一律 false：文件不存在或解析失败时按关处理，
/// 避免一次写坏配置就让后台代理偷偷记用量 / 写日志。
fn read_cli_config() -> serde_json::Value {
    fs::read_to_string(cli_config_path())
        .ok()
        .and_then(|text| serde_json::from_str::<serde_json::Value>(&text).ok())
        .filter(|v| v.is_object())
        .unwrap_or_else(|| serde_json::json!({}))
}

fn cli_config_bool(key: &str) -> bool {
    read_cli_config()
        .get(key)
        .and_then(|x| x.as_bool())
        .unwrap_or(false)
}

/// 合并写入，避免 `proxy usage` / `proxy log` 互相抹掉对方的配置。
fn set_cli_config_bool(key: &str, enabled: bool) {
    let mut cfg = read_cli_config();
    let obj = cfg
        .as_object_mut()
        .expect("read_cli_config 保证返回 object");
    obj.insert(key.to_string(), serde_json::Value::Bool(enabled));
    obj.insert(
        "note".to_string(),
        serde_json::json!("仅对 CLI 自己的代理进程生效，GUI 不受影响"),
    );
    let text = serde_json::to_string_pretty(&cfg).unwrap_or_else(|_| "{}".to_string());
    if let Err(e) = fs::write(cli_config_path(), format!("{text}\n")) {
        err(&format!("写 {} 失败: {e}", cli_config_path().display()));
    }
}

fn usage_recording_enabled() -> bool {
    cli_config_bool("usage_recording")
}

/// 后台代理日志开关，默认关：不创建 cli-proxy.log。
fn proxy_log_enabled() -> bool {
    cli_config_bool("proxy_log")
}

fn set_usage_recording(enabled: bool) {
    set_cli_config_bool("usage_recording", enabled);
    println!(
        "用量记录: {}（仅对本 CLI 代理生效，下次 proxy enable 起效）",
        if enabled { "开" } else { "关" }
    );
}

fn set_proxy_log(enabled: bool) {
    set_cli_config_bool("proxy_log", enabled);
    println!(
        "后台代理日志: {}（{}）",
        if enabled { "开" } else { "关（默认）" },
        if enabled {
            format!("写入 {}", proxy_log_path().display())
        } else {
            "不创建 cli-proxy.log".to_string()
        }
    );
    println!("下次 proxy enable 起效；已运行的代理需 proxy disable 后重新 enable");
}

fn parse_bool(s: &str) -> Result<bool, String> {
    match s.trim().to_ascii_lowercase().as_str() {
        "true" | "1" | "yes" | "on" => Ok(true),
        "false" | "0" | "no" | "off" => Ok(false),
        other => Err(format!("布尔值只能是 true/false，得到: {other}")),
    }
}

/// 终端显示列宽：CJK / 全角 / emoji 占 2 列，组合符号和控制字符不占列。
/// 只用于把 `provider list` 的列对齐，不引入 unicode-width 依赖。
fn char_width(c: char) -> usize {
    let u = c as u32;
    if u == 0 || u < 0x20 || (0x7f..0xa0).contains(&u) {
        return 0;
    }
    if (0x0300..=0x036f).contains(&u)
        || (0x1ab0..=0x1aff).contains(&u)
        || (0x1dc0..=0x1dff).contains(&u)
        || (0xfe00..=0xfe0f).contains(&u)
        || (0xfe20..=0xfe2f).contains(&u)
    {
        return 0;
    }
    if (0x1100..=0x115f).contains(&u)
        || (0x2e80..=0x303e).contains(&u)
        || (0x3041..=0x33ff).contains(&u)
        || (0x3400..=0x4dbf).contains(&u)
        || (0x4e00..=0x9fff).contains(&u)
        || (0xa000..=0xa4cf).contains(&u)
        || (0xac00..=0xd7a3).contains(&u)
        || (0xf900..=0xfaff).contains(&u)
        || (0xfe10..=0xfe19).contains(&u)
        || (0xfe30..=0xfe6f).contains(&u)
        || (0xff00..=0xff60).contains(&u)
        || (0xffe0..=0xffe6).contains(&u)
        || (0x1f300..=0x1faff).contains(&u)
        || (0x20000..=0x3fffd).contains(&u)
    {
        return 2;
    }
    1
}

fn display_width(s: &str) -> usize {
    s.chars().map(char_width).sum()
}

fn pad_display(s: &str, width: usize) -> String {
    format!("{s}{}", " ".repeat(width.saturating_sub(display_width(s))))
}

/// CLI 代理状态事件的 sink：没有前端可推，写到 stderr（低频事件）。
///
/// 用量写入通知走单独的 [`QuietEventSink`]：它每条日志都会触发（200ms
/// 防抖），打到 stderr 会淹没 `proxy enable` 之后守护进程的日志。
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
/// CLI 的 `proxy enable` 拉起的是后台进程，事件打 stderr 会写满日志文件，所以走
/// log（CLI 目前没装 logger，等于丢弃；将来接上 logger 即生效）。
struct QuietEventSink;

impl EventSink for QuietEventSink {
    fn emit(&self, event: &str, _payload: serde_json::Value) {
        // 走 log 而不是 stderr：后台进程每条日志（200ms 防抖）打一行会迅速
        // 写满 cli-proxy.log。CLI 目前没装 logger，因此这条 debug
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
    match try_open_state() {
        Ok(state) => state,
        Err(e) => err(&e),
    }
}

/// `proxy enable` / `disable` 的客户端收尾用：代理已经起/停成功了，这时数据库打不开
/// 也不能把整条命令判成失败，只能放弃重写客户端配置并给出警告。
fn try_open_state() -> Result<Arc<AppState>, String> {
    let db = Arc::new(Database::init().map_err(|e| format!("打开数据库失败: {e}"))?);
    let state = Arc::new(AppState::new(db));
    // 不注入钩子时 ProxyService 用 FrontendHooks::default()：故障转移永不执行、
    // OAuth 路由直接报错、事件静默丢弃。CLI 要的是和 GUI 不同的最小实现，
    // 不是空实现。
    state.proxy_service.set_hooks(cli_hooks(&state));
    // 用量写入通知是另一套 OnceLock sink（不在 ProxyService.hooks 里），
    // 不单独注入就会整条链路静默。
    cc_switch_core::usage_events::init(Arc::new(QuietEventSink));
    Ok(state)
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
        print!("{}", help());
        return;
    }
    if matches!(args[0].as_str(), "version" | "--version" | "-V") {
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

    // enable / disable 自己控制开库时机：先起/停代理，再做客户端接上/分离，
    // 不走下面统一 open_state() 的命令分派。
    if args[0] == "proxy" {
        if let Some(action @ ("enable" | "disable")) = args.get(1).map(|s| s.as_str()) {
            cmd_proxy_control(app, action);
            return;
        }
    }

    let state = open_state();
    match args[0].as_str() {
        "provider" => cmd_provider(&state, app, &args[1..]),
        "use" => {
            if app != AppType::Codex {
                err("agy 不支持 switch：卡片只作用于它打开的终端，请用 `agy launch <id>`");
            }
            let id = args
                .get(1)
                .unwrap_or_else(|| err("用法: use <id> [--no-attach]"));
            cmd_switch(&state, app, id, parse_switch_flags(&args[2..]));
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
            // 按显示宽度补齐再输出：原来用 \t 分隔，id 长短不一 + 中文名称时
            // 名称列会错位。当前供应商单独占一列，避免 " *" 又把 id 列推歪。
            let id_width = list
                .keys()
                .map(|id| display_width(id))
                .max()
                .unwrap_or(0)
                .max(display_width("ID"));
            println!(
                "{}  {}  名称",
                pad_display("当前", 4),
                pad_display("ID", id_width)
            );
            println!(
                "{}  {}  {}",
                "-".repeat(4),
                "-".repeat(id_width),
                "-".repeat(4)
            );
            for (id, p) in &list {
                let mark = if *id == current { "*" } else { "" };
                println!(
                    "{}  {}  {}",
                    pad_display(mark, 4),
                    pad_display(id, id_width),
                    p.name
                );
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
                .unwrap_or_else(|| err("用法: provider switch <id> [--no-attach]"));
            cmd_switch(state, app, id, parse_switch_flags(&args[2..]));
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

/// `switch` / `use` 后面的旗标：只认 `--no-attach`（路由 / 聚合模式下切换后不接上客户端，
/// 只记状态，客户端文件和模型目录等 CC Switch 下次启动时才更新），别的参数一律报错——
/// 原来多余参数被静默忽略，`--no-atatch` 这种拼错会变成「照样接了客户端」。
fn parse_switch_flags(flags: &[String]) -> bool {
    let mut attach = true;
    for flag in flags {
        match flag.as_str() {
            "--no-attach" => attach = false,
            other => err(&format!(
                "switch 不认识参数 {other}；用法: provider switch <id> [--no-attach]"
            )),
        }
    }
    attach
}

fn cmd_switch(state: &AppState, app: AppType, id: &str, attach: bool) {
    match ProviderService::switch(state, app.clone(), id) {
        Ok(result) => {
            println!("已切到 {id}");
            for w in &result.warnings {
                println!("警告: {w}");
            }
            if attach {
                ensure_client_live(state, &app);
            }
        }
        Err(e) => err(&format!("切换失败: {e}")),
    }
}

/// 代理端口上是不是已经有人在服务。GUI 内嵌的代理和本 CLI 的守护进程用同一个端口，
/// 只看 pid 文件会漏掉「GUI 正在跑」这种情况，进而拉起一个抢同一端口的守护进程
/// （bind 失败，切换刚成功就报一串启动失败）。
fn proxy_port_serving(rt: &tokio::runtime::Runtime, state: &AppState) -> bool {
    let Ok(cfg) = rt.block_on(state.proxy_service.get_config()) else {
        return false;
    };
    if cfg.listen_port == 0 {
        return false;
    }
    let host = match cfg.listen_address.trim() {
        "" | "0.0.0.0" | "::" | "[::]" => "127.0.0.1",
        other => other,
    };
    let Ok(addrs) = (host, cfg.listen_port).to_socket_addrs() else {
        return false;
    };
    addrs
        .into_iter()
        .any(|addr| TcpStream::connect_timeout(&addr, Duration::from_millis(300)).is_ok())
}

/// 路由 / 聚合模式下，CC Switch 没运行时客户端文件是分离回直连的那一份：切换供应商只落定
/// 状态（`mode::controller::switch_route_locked`），`config.toml` 和
/// `cc-switch-model-catalog.json` 都不动，Codex 的模型列表因此停在旧的那几个。
///
/// 两种情况下客户端得有人管：
/// - 已分离：按 GUI 启动时的做法接上，重写客户端文件（含模型目录）；
/// - 已接上但代理没人服务（比如重启过机器）：客户端还指着代理端口，得把代理拉回来。
///
/// 客户端一旦指向代理，代理就必须在跑，否则每个请求都打到没人监听的端口，所以先确保代理。
fn ensure_client_live(state: &AppState, app: &AppType) {
    if !app.has_route_mode() || !cc_switch_core::mode::current::is_proxy(app) {
        return;
    }
    let attached = cc_switch_core::mode::current::mode_state(app).attached;
    let rt = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(rt) => rt,
        Err(e) => {
            println!("警告: 建运行时失败（{e}），客户端配置保持分离");
            return;
        }
    };
    if running_proxy_pid().is_none() && !proxy_port_serving(&rt, state) {
        match start_proxy_daemon(app, proxy_log_enabled()) {
            Ok(pid) => println!("后台代理已启动（pid {pid}）"),
            Err(e) => {
                println!(
                    "警告: 拉起后台代理失败（{e}）。客户端配置保持分离，先 `proxy enable` 再切换才会更新"
                );
                return;
            }
        }
    }
    // 已接上时客户端文件本来就指着代理，只差代理在跑。
    if attached {
        return;
    }
    match rt.block_on(cc_switch_core::mode::controller::attach_files(state, app)) {
        Ok(()) => println!(
            "已接上代理：config.toml 与 cc-switch-model-catalog.json 已按当前路由 / 聚合重写"
        ),
        Err(e) => println!("警告: 接上客户端配置失败: {e}"),
    }
}

/// 把 `--key value` 选项合进已有的 codexChatReasoning。
///
/// 落地规则跟 GUI 表单一致（`ProviderForm.tsx` 里 codexChatReasoning 的归一化）：
/// - 关思考 → `thinkingParam = "none"`（#8970：别让卡里残留的 thinking 继续注入）；
/// - 关档位 → `effortParam = "none"`，且不带 `effortValueMode`；
/// - 开档位 → 缺省补 `reasoning_effort`（已声明 `reasoning.effort` 时保留）/ `passthrough`。
///
/// 显式给的 `--thinking-param` / `--effort-param` / `--effort-value-mode` 压过归一化。
/// 出错返回 Err 而不是直接退出，方便单测。
fn merge_reasoning_options(
    reasoning: &mut serde_json::Value,
    opts: &[String],
) -> Result<(), String> {
    let mut supports_thinking: Option<bool> = None;
    let mut supports_effort: Option<bool> = None;
    let mut thinking_param: Option<String> = None;
    let mut effort_param: Option<String> = None;
    let mut effort_value_mode: Option<String> = None;
    let mut output_format: Option<String> = None;

    let mut i = 0;
    while i < opts.len() {
        let (key, value) = match (opts.get(i), opts.get(i + 1)) {
            (Some(key), Some(value)) => (key.as_str(), value.as_str()),
            _ => {
                return Err(format!(
                    "reasoning 选项要成对出现，例如 --thinking-param none；多出来的是 {}",
                    opts.get(i).map(|s| s.as_str()).unwrap_or("")
                ))
            }
        };
        // 下面几个枚举和 GUI 的类型逐字对齐（src/types.ts 的 CodexChat* 一组）。
        match key {
            "--supports-thinking" => supports_thinking = Some(parse_bool(value)?),
            "--supports-effort" => supports_effort = Some(parse_bool(value)?),
            "--thinking-param" => {
                if !["thinking", "enable_thinking", "reasoning_split", "none"].contains(&value) {
                    return Err(concat!(
                        "thinking-param 只能是 thinking / enable_thinking / ",
                        "reasoning_split / none"
                    )
                    .to_string());
                }
                thinking_param = Some(value.to_string());
            }
            "--effort-param" => {
                if !["reasoning_effort", "reasoning.effort", "none"].contains(&value) {
                    return Err(
                        "effort-param 只能是 reasoning_effort / reasoning.effort / none"
                            .to_string(),
                    );
                }
                effort_param = Some(value.to_string());
            }
            "--effort-value-mode" => {
                if !["passthrough", "low_high", "deepseek", "openrouter", "zen"].contains(&value) {
                    return Err(concat!(
                        "effort-value-mode 只能是 passthrough / low_high / deepseek / ",
                        "openrouter / zen"
                    )
                    .to_string());
                }
                effort_value_mode = Some(value.to_string());
            }
            "--output-format" => {
                if ![
                    "auto",
                    "reasoning_content",
                    "reasoning",
                    "reasoning_details",
                    "think_tags",
                ]
                .contains(&value)
                {
                    return Err(concat!(
                        "output-format 只能是 auto / reasoning_content / reasoning / ",
                        "reasoning_details / think_tags"
                    )
                    .to_string());
                }
                output_format = Some(value.to_string());
            }
            other => return Err(format!("未知 reasoning 选项: {other}")),
        }
        i += 2;
    }

    // 库里存的不是对象（比如被手改成 null）时，按空对象重新开始，别在下面索引处 panic。
    if !reasoning.is_object() {
        *reasoning = serde_json::json!({});
    }
    // #8970 解耦：绝不由 effort 派生 thinking；显式 param 优先。
    if let Some(v) = supports_thinking {
        reasoning["supportsThinking"] = serde_json::json!(v);
    }
    if let Some(v) = supports_effort {
        reasoning["supportsEffort"] = serde_json::json!(v);
    }
    if let Some(v) = thinking_param {
        reasoning["thinkingParam"] = serde_json::json!(v);
    } else if supports_thinking == Some(false) {
        reasoning["thinkingParam"] = serde_json::json!("none");
    } else if supports_thinking == Some(true) && reasoning.get("thinkingParam").is_none() {
        reasoning["thinkingParam"] = serde_json::json!("thinking");
    }
    if let Some(v) = effort_param {
        reasoning["effortParam"] = serde_json::json!(v);
    } else if let Some(enabled) = supports_effort {
        if enabled {
            let keep_openrouter =
                reasoning.get("effortParam").and_then(|v| v.as_str()) == Some("reasoning.effort");
            reasoning["effortParam"] = serde_json::json!(if keep_openrouter {
                "reasoning.effort"
            } else {
                "reasoning_effort"
            });
        } else {
            reasoning["effortParam"] = serde_json::json!("none");
        }
    }
    if let Some(v) = effort_value_mode {
        reasoning["effortValueMode"] = serde_json::json!(v);
    } else if supports_effort == Some(false) {
        if let Some(obj) = reasoning.as_object_mut() {
            obj.remove("effortValueMode");
        }
    } else if supports_effort == Some(true) && reasoning.get("effortValueMode").is_none() {
        reasoning["effortValueMode"] = serde_json::json!("passthrough");
    }
    if let Some(v) = output_format {
        reasoning["outputFormat"] = serde_json::json!(v);
    }
    Ok(())
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
    if let Err(e) = merge_reasoning_options(&mut reasoning, opts) {
        err(&e);
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

/// 把 `<私有目录>/config` 指到共享的 `~/.gemini/config`（上游规则）。
///
/// 已是真实目录时不动它；建链接失败只警告，不拦 launch —— 原来的实现里 Windows
/// 分支被 `#[cfg(unix)]` 整段吃掉，失败一声不吭，agy 静默读不到共享配置。
fn link_shared_dir(link: &Path, shared: &Path) {
    match fs::symlink_metadata(link) {
        Ok(meta) if meta.file_type().is_symlink() => {
            if fs::read_link(link).ok().as_deref() == Some(shared) {
                return;
            }
            // 指向别处：摘掉旧的再建（junction 要用 remove_dir）。
            if let Err(e) = fs::remove_file(link).or_else(|_| fs::remove_dir(link)) {
                println!("警告: 无法移除旧的链接 {}: {e}", link.display());
                return;
            }
        }
        Ok(_) => {
            println!("注意：{} 已是真实目录，按上游规则不动它", link.display());
            return;
        }
        Err(_) => {}
    }
    if let Err(e) = create_dir_link(shared, link) {
        println!(
            "警告: 建目录链接 {} -> {} 失败（{e}）；agy 可能读不到共享配置",
            link.display(),
            shared.display()
        );
    }
}

#[cfg(unix)]
fn create_dir_link(shared: &Path, link: &Path) -> std::io::Result<()> {
    std::os::unix::fs::symlink(shared, link)
}

/// Windows 上先试符号链接（要管理员或开发者模式），失败退回 junction：
/// `mklink /J` 普通权限就能建，agy 只是当目录读，够用。
#[cfg(windows)]
fn create_dir_link(shared: &Path, link: &Path) -> std::io::Result<()> {
    match std::os::windows::fs::symlink_dir(shared, link) {
        Ok(()) => Ok(()),
        Err(symlink_err) => {
            let status = Command::new("cmd")
                .args(["/C", "mklink", "/J"])
                .arg(link)
                .arg(shared)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status();
            match status {
                Ok(s) if s.success() => Ok(()),
                _ => Err(symlink_err),
            }
        }
    }
}

#[cfg(not(any(unix, windows)))]
fn create_dir_link(_shared: &Path, _link: &Path) -> std::io::Result<()> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "当前平台不支持目录链接",
    ))
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
        // <私有目录>/config 指回共享的 ~/.gemini/config；已是真实目录时不动它。
        // 用核心的 get_home_dir：Windows 上没有 HOME，原来会退化成 "." 并在当前目录
        // 建出一份假的共享目录。
        let shared = cc_switch_core::config::get_home_dir()
            .join(".gemini")
            .join("config");
        if let Err(e) = fs::create_dir_all(&shared) {
            println!("警告: 建共享配置目录 {} 失败: {e}", shared.display());
        }
        link_shared_dir(&dir.join("config"), &shared);
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
                            "  应用: {}（本 CLI 的 codex 代理由 proxy enable 后台运行；agy 不走代理）",
                            app.as_str()
                        );
                        // GUI 内嵌的代理也占同一个端口，只看 pid 文件会误报“未运行”。
                        println!(
                            "  后台状态: {}",
                            match running_proxy_pid() {
                                Some(pid) => format!("运行中（pid {pid}）"),
                                None if proxy_port_serving(&rt, state) => format!(
                                    "{}:{} 已在服务（通常是 GUI 在跑，本 CLI 没拉起守护进程）",
                                    cfg.listen_address, cfg.listen_port
                                ),
                                None => "未运行".to_string(),
                            }
                        );
                        println!(
                            "  用量记录（CLI 专属）: {}",
                            if usage_recording_enabled() {
                                "开"
                            } else {
                                "关（默认）"
                            }
                        );
                        println!(
                            "  后台代理日志: {}",
                            if proxy_log_enabled() {
                                format!("开（{}）", proxy_log_path().display())
                            } else {
                                "关（默认，不写 cli-proxy.log）".to_string()
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
        Some("log") => match args.get(1).map(|s| s.as_str()) {
            Some("on") => set_proxy_log(true),
            Some("off") => set_proxy_log(false),
            _ => println!(
                "后台代理日志: {}",
                if proxy_log_enabled() {
                    "开"
                } else {
                    "关（默认）"
                }
            ),
        },
        Some(PROXY_DAEMON_CMD) => run_proxy_daemon(state, app),
        _ => err("proxy 只支持 show / enable / disable / usage [on|off] / log [on|off]"),
    }
}

/// 后台代理本体：只由 `proxy enable` 通过内部命令拉起；是否写 cli-proxy.log
/// 由 cli.json 的 proxy_log 开关决定（默认不写）。
/// 退出（SIGTERM / Ctrl-C）时主动停代理并清掉自己的 pid 文件。
fn run_proxy_daemon(state: &AppState, app: AppType) {
    if app != AppType::Codex {
        err("后台代理只适用于 codex；agy 不走代理，请用 agy launch");
    }
    let pid = std::process::id();
    write_proxy_pid(pid);

    let enabled = usage_recording_enabled();
    cc_switch_core::set_cli_usage_recording(enabled);
    // 长驻进程：和 GUI 一样启动 WebDAV/S3 自动同步 worker（远端没配置时
    // worker 自己空转）。一次性命令不启，短进程没必要拉起后台 runtime。
    cc_switch_core::services::webdav_auto_sync::start_worker(
        state.db.clone(),
        Arc::new(QuietEventSink),
    );
    cc_switch_core::services::s3_auto_sync::start_worker(
        state.db.clone(),
        Arc::new(QuietEventSink),
    );
    println!(
        "启动 codex 后台代理（pid {pid}；用量记录: {}，仅对本进程生效）",
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
    // enable 靠这行判定「已就绪」，别改掉前缀。
    println!("代理已起: {}:{}", info.address, info.port);

    rt.block_on(wait_for_proxy_shutdown());
    let _ = rt.block_on(state.proxy_service.stop());
    if read_proxy_pid() == Some(pid) {
        remove_proxy_pid();
    }
    println!("代理已停");
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn display_width_counts_cjk_as_two_columns() {
        assert_eq!(display_width("abc"), 3);
        assert_eq!(display_width("中文"), 4);
        assert_eq!(display_width("a中b"), 4);
        assert_eq!(display_width(""), 0);
    }

    #[test]
    fn padding_aligns_mixed_width_ids() {
        assert_eq!(pad_display("ab", 6), "ab    ");
        assert_eq!(pad_display("中文", 6), "中文  ");
        assert_eq!(display_width(&pad_display("中文", 6)), 6);
    }

    fn opts(pairs: &[&str]) -> Vec<String> {
        pairs.iter().map(|s| s.to_string()).collect()
    }

    /// reasoning 选项的落地规则要和 GUI 表单一致，且坏值一律拒绝。
    #[test]
    fn reasoning_options_follow_the_form_rules() {
        // 关思考：落 none，不注入 thinking（#8970）。
        let mut r = serde_json::json!({ "supportsThinking": true, "thinkingParam": "thinking" });
        merge_reasoning_options(&mut r, &opts(&["--supports-thinking", "false"])).unwrap();
        assert_eq!(r["supportsThinking"], false);
        assert_eq!(r["thinkingParam"], "none");

        // 开思考且缺省：补 thinking。
        let mut r = serde_json::json!({});
        merge_reasoning_options(&mut r, &opts(&["--supports-thinking", "true"])).unwrap();
        assert_eq!(r["thinkingParam"], "thinking");

        // 关档位：effortParam=none，并且不带 effortValueMode（GUI 也是这么落库）。
        let mut r = serde_json::json!({
            "supportsEffort": true,
            "effortParam": "reasoning_effort",
            "effortValueMode": "deepseek"
        });
        merge_reasoning_options(&mut r, &opts(&["--supports-effort", "off"])).unwrap();
        assert_eq!(r["effortParam"], "none");
        assert!(r.get("effortValueMode").is_none(), "{r}");

        // 开档位且缺省：补 reasoning_effort / passthrough；已声明 reasoning.effort 的保留。
        let mut r = serde_json::json!({ "supportsEffort": false });
        merge_reasoning_options(&mut r, &opts(&["--supports-effort", "on"])).unwrap();
        assert_eq!(r["effortParam"], "reasoning_effort");
        assert_eq!(r["effortValueMode"], "passthrough");
        let mut r = serde_json::json!({
            "effortParam": "reasoning.effort",
            "effortValueMode": "openrouter"
        });
        merge_reasoning_options(&mut r, &opts(&["--supports-effort", "true"])).unwrap();
        assert_eq!(r["effortParam"], "reasoning.effort");
        assert_eq!(r["effortValueMode"], "openrouter");

        // 显式参数压过归一化。
        let mut r = serde_json::json!({});
        merge_reasoning_options(
            &mut r,
            &opts(&[
                "--supports-effort",
                "false",
                "--effort-param",
                "reasoning_effort",
            ]),
        )
        .unwrap();
        assert_eq!(r["effortParam"], "reasoning_effort");

        // output-format 合法值写入，非法值/未知选项/不成对都要报错。
        let mut r = serde_json::json!({});
        merge_reasoning_options(&mut r, &opts(&["--output-format", "think_tags"])).unwrap();
        assert_eq!(r["outputFormat"], "think_tags");
        assert!(merge_reasoning_options(&mut r, &opts(&["--output-format", "xml"])).is_err());
        assert!(merge_reasoning_options(&mut r, &opts(&["--nope", "x"])).is_err());
        assert!(merge_reasoning_options(&mut r, &opts(&["--output-format"])).is_err());
        assert!(merge_reasoning_options(&mut r, &opts(&["--supports-effort", "maybe"])).is_err());
        assert!(merge_reasoning_options(&mut r, &opts(&["--effort-value-mode", "bogus"])).is_err());

        // 库里存的不是对象也不该 panic。
        let mut r = serde_json::Value::Null;
        merge_reasoning_options(&mut r, &opts(&["--supports-thinking", "false"])).unwrap();
        assert_eq!(r["thinkingParam"], "none");
    }

    /// 私有目录的 config 要指到共享目录；已是真实目录时不动；指向别处时改指。
    #[test]
    fn link_shared_dir_points_to_the_shared_dir_and_leaves_a_real_dir_alone() {
        let root = env::temp_dir().join(format!("ccsw-link-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        let shared = root.join("shared");
        let other = root.join("other");
        let link = root.join("private").join("config");
        fs::create_dir_all(&shared).unwrap();
        fs::create_dir_all(&other).unwrap();
        fs::create_dir_all(link.parent().unwrap()).unwrap();

        // 建链接：能读到共享目录里的东西。
        link_shared_dir(&link, &shared);
        assert!(
            fs::symlink_metadata(&link)
                .map(|meta| meta.file_type().is_symlink())
                .unwrap_or(false),
            "{} 应该是链接",
            link.display()
        );
        fs::write(shared.join("probe.json"), "{}").unwrap();
        assert!(link.join("probe.json").exists());

        // 已是真实目录：按上游规则不动它。
        fs::remove_file(&link)
            .or_else(|_| fs::remove_dir(&link))
            .unwrap();
        fs::create_dir_all(&link).unwrap();
        link_shared_dir(&link, &shared);
        assert!(!fs::symlink_metadata(&link)
            .unwrap()
            .file_type()
            .is_symlink());

        // 指向别处：改指回共享目录。
        fs::remove_dir(&link).unwrap();
        link_shared_dir(&link, &other);
        assert_eq!(fs::read_link(&link).unwrap(), other);
        link_shared_dir(&link, &shared);
        assert_eq!(fs::read_link(&link).unwrap(), shared);

        let _ = fs::remove_dir_all(&root);
    }
}
