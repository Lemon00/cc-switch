#![allow(non_snake_case)]

use crate::app_config::AppType;
use crate::init_status::{InitErrorPayload, SkillsMigrationPayload};
use crate::services::ProviderService;
use indexmap::IndexSet;
use regex::Regex;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::str::FromStr;
use std::sync::Arc;
use std::sync::LazyLock;
use tauri::AppHandle;
use tauri::State;
use tauri_plugin_opener::OpenerExt;

#[path = "tool_node_requirement.rs"]
mod tool_node_requirement;
#[path = "tool_uninstall.rs"]
mod tool_uninstall;
#[path = "tool_discovery.rs"]
pub(super) mod tool_discovery;
#[path = "tool_lifecycle_command.rs"]
mod tool_lifecycle_command;
#[path = "tool_process.rs"]
pub(super) mod tool_process;
#[path = "tool_version.rs"]
pub(super) mod tool_version;
use tool_discovery::*;
use tool_lifecycle_command::*;
use tool_process::*;
use tool_version::*;

#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;

#[cfg(target_os = "windows")]
const CREATE_NO_WINDOW: u32 = 0x08000000;

/// 本 fork 的发布仓库：updater 清单、便携版安装包、发布说明都以它为基准。
/// 与 tauri.conf.json 的 plugins.updater.endpoints 保持一致。
const REPO_URL: &str = "https://github.com/Lemon00/cc-switch";

/// 打开外部链接
/// 外部链接白名单：只允许 `http:` / `https:` / `mailto:`。
/// 不带协议的裸域名（如 `example.com/docs`）按旧行为补 `https://`；
/// `javascript:`、`file:`、`data:` 及其他自定义协议一律拒绝（前端已过滤，这里是第二道）。
fn validate_external_url(raw: &str) -> Result<String, String> {
    let raw = raw.trim();
    let parsed = match url::Url::parse(raw) {
        Ok(parsed) => parsed,
        Err(url::ParseError::RelativeUrlWithoutBase) => {
            url::Url::parse(&format!("https://{raw}")).map_err(|_| "链接格式无效".to_string())?
        }
        Err(_) => return Err("链接格式无效".to_string()),
    };
    match parsed.scheme() {
        "http" | "https" if parsed.host_str().is_some_and(|h| !h.is_empty()) => Ok(parsed.into()),
        "mailto" => Ok(parsed.into()),
        "http" | "https" => Err("链接格式无效".to_string()),
        _ => Err("只能打开 http、https 或 mailto 链接".to_string()),
    }
}

#[tauri::command]
pub async fn open_external(app: AppHandle, url: String) -> Result<bool, String> {
    let url = validate_external_url(&url)?;

    app.opener()
        .open_url(&url, None::<String>)
        .map_err(|e| format!("打开链接失败: {e}"))?;

    Ok(true)
}

#[tauri::command]
pub async fn copy_text_to_clipboard(text: String) -> Result<bool, String> {
    // Use spawn_blocking to avoid blocking the async runtime
    // Clipboard access can block on some platforms and may have thread/loop constraints
    tokio::task::spawn_blocking(move || {
        let mut clipboard =
            arboard::Clipboard::new().map_err(|e| format!("访问系统剪贴板失败: {e}"))?;
        clipboard
            .set_text(text)
            .map_err(|e| format!("写入系统剪贴板失败: {e}"))?;
        Ok(true)
    })
    .await
    .map_err(|e| format!("剪贴板任务执行失败: {e}"))?
}

#[tauri::command]
pub async fn check_for_updates(handle: AppHandle) -> Result<bool, String> {
    handle
        .opener()
        .open_url(format!("{REPO_URL}/releases/latest"), None::<String>)
        .map_err(|e| format!("打开更新页面失败: {e}"))?;

    Ok(true)
}

/// 便携版新版本压缩包的下载地址，命名与 release.yml 的「Prepare Windows Assets」一致：
/// `CC-Switch-v<版本>-Windows[-arm64]-Portable.zip`。版本号不像版本号时返回 None。
fn portable_zip_url(version: &str, arm64: bool) -> Option<String> {
    let version = version.trim().trim_start_matches('v');
    let looks_like_version = version.starts_with(|c: char| c.is_ascii_digit())
        && version
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '-' | '+'));
    if !looks_like_version {
        return None;
    }
    let suffix = if arm64 { "-arm64" } else { "" };
    Some(format!(
        "{REPO_URL}/releases/download/v{version}/CC-Switch-v{version}-Windows{suffix}-Portable.zip"
    ))
}

/// 便携版更新：直接下载对应架构的新版压缩包（解压覆盖即可，数据在 ~/.cc-switch 不受影响）。
/// 不做原地替换自身 exe：那样很容易被杀毒软件当成病毒拦截。
#[tauri::command]
pub async fn open_portable_update_download(
    handle: AppHandle,
    version: String,
) -> Result<bool, String> {
    let url = portable_zip_url(&version, cfg!(target_arch = "aarch64"))
        .unwrap_or_else(|| format!("{REPO_URL}/releases/latest"));
    handle
        .opener()
        .open_url(url, None::<String>)
        .map_err(|e| format!("打开更新页面失败: {e}"))?;

    Ok(true)
}

/// 判断是否为便携版（绿色版）运行
#[tauri::command]
pub async fn is_portable_mode() -> Result<bool, String> {
    let exe_path = std::env::current_exe().map_err(|e| format!("获取可执行路径失败: {e}"))?;
    if let Some(dir) = exe_path.parent() {
        Ok(dir.join("portable.ini").is_file())
    } else {
        Ok(false)
    }
}

/// 获取应用启动阶段的初始化错误（若有）。
/// 用于前端在早期主动拉取，避免事件订阅竞态导致的提示缺失。
#[tauri::command]
pub async fn get_init_error() -> Result<Option<InitErrorPayload>, String> {
    Ok(crate::init_status::get_init_error())
}

/// 获取 JSON→SQLite 迁移结果（若有）。
/// 只返回一次 true，之后返回 false，用于前端显示一次性 Toast 通知。
#[tauri::command]
pub async fn get_migration_result() -> Result<bool, String> {
    Ok(crate::init_status::take_migration_success())
}

/// 获取 Skills 自动导入（SSOT）迁移结果（若有）。
/// 只返回一次 Some({count})，之后返回 None，用于前端显示一次性 Toast 通知。
#[tauri::command]
pub async fn get_skills_migration_result() -> Result<Option<SkillsMigrationPayload>, String> {
    Ok(crate::init_status::take_skills_migration_result())
}

#[derive(serde::Serialize)]
pub struct ToolVersion {
    name: String,
    version: Option<String>,
    latest_version: Option<String>, // 新增字段：最新版本
    error: Option<String>,
    /// 已定位到可执行文件、但 `--version` 报错退出（装了却跑不起来，如 Node 版本不达标）。
    /// 供前端区分"未安装"与"已安装·无法运行"，无需匹配 error 文案反推语义。
    installed_but_broken: bool,
    /// 工具运行环境: "windows", "wsl", "macos", "linux", "unknown"
    env_type: String,
    /// 当 env_type 为 "wsl" 时，返回该工具绑定的 WSL distro（用于按 distro 探测 shells）
    wsl_distro: Option<String>,
    /// 有新版本、但新版本要求的 Node 比这处安装用的新：前端不给升级按钮，提示先升级 Node。
    update_blocked_by_node: Option<tool_node_requirement::NodeRequirementGap>,
}

const VALID_TOOLS: [&str; 13] = [
    "claude", "codex", "gemini", "grok", "opencode", "openclaw", "hermes", "pi", "mcode", "kimi",
    "dsh", "agy", "omp",
];

#[derive(Debug, Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WslShellPreferenceInput {
    #[serde(default)]
    pub wsl_shell: Option<String>,
    #[serde(default)]
    pub wsl_shell_flag: Option<String>,
}

// Keep platform-specific env detection in one place to avoid repeating cfg blocks.
#[cfg(target_os = "windows")]
fn tool_env_type_and_wsl_distro(tool: &str) -> (String, Option<String>) {
    if let Some(distro) = wsl_distro_for_tool(tool) {
        ("wsl".to_string(), Some(distro))
    } else {
        ("windows".to_string(), None)
    }
}

#[cfg(target_os = "macos")]
fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("macos".to_string(), None)
}

#[cfg(target_os = "linux")]
fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("linux".to_string(), None)
}

#[cfg(not(any(target_os = "windows", target_os = "macos", target_os = "linux")))]
fn tool_env_type_and_wsl_distro(_tool: &str) -> (String, Option<String>) {
    ("unknown".to_string(), None)
}

#[tauri::command]
pub async fn get_tool_versions(
    tools: Option<Vec<String>>,
    wsl_shell_by_tool: Option<HashMap<String, WslShellPreferenceInput>>,
) -> Result<Vec<ToolVersion>, String> {
    let requested: Vec<&str> = if let Some(tools) = tools.as_ref() {
        let set: std::collections::HashSet<&str> = tools.iter().map(|s| s.as_str()).collect();
        VALID_TOOLS
            .iter()
            .copied()
            .filter(|t| set.contains(t))
            .collect()
    } else {
        VALID_TOOLS.to_vec()
    };
    let mut results = Vec::new();

    for tool in requested {
        let pref = wsl_shell_by_tool.as_ref().and_then(|m| m.get(tool));
        let tool_wsl_shell = pref.and_then(|p| p.wsl_shell.as_deref());
        let tool_wsl_shell_flag = pref.and_then(|p| p.wsl_shell_flag.as_deref());

        results.push(get_single_tool_version_impl(tool, tool_wsl_shell, tool_wsl_shell_flag).await);
    }

    Ok(results)
}

// 不同工具仍可能共用 pnpm/npm 的全局目录。先保守地串行化所有安装写入，
// 并在排队前锁定工具，防止页面重挂或另一 IPC 调用重复提交同一工具。
struct ToolLifecycleCoordinator {
    tools: HashMap<&'static str, Arc<tokio::sync::Mutex<()>>>,
    execution: Arc<tokio::sync::Mutex<()>>,
}

impl Default for ToolLifecycleCoordinator {
    fn default() -> Self {
        Self {
            tools: VALID_TOOLS
                .iter()
                .map(|&tool| (tool, Arc::new(tokio::sync::Mutex::new(()))))
                .collect(),
            execution: Arc::new(tokio::sync::Mutex::new(())),
        }
    }
}

impl ToolLifecycleCoordinator {
    async fn run<F>(&self, tools: Vec<&'static str>, operation: F) -> Result<(), String>
    where
        F: FnOnce(&[&str]) -> Result<(), String> + Send + 'static,
    {
        let tool_guards = tools
            .iter()
            .map(|tool| {
                self.tools
                    .get(tool)
                    .ok_or_else(|| format!("Unsupported tool action target: {tool}"))?
                    .clone()
                    .try_lock_owned()
                    // 稳定错误码供前端区分后台任务仍在进行与真正的执行失败。
                    .map_err(|_| "TOOL_ACTION_IN_PROGRESS".to_string())
            })
            .collect::<Result<Vec<_>, _>>()?;
        let execution_guard = self.execution.clone().lock_owned().await;

        // 必须把锁移进 blocking 任务：即使等待它的 IPC future 被取消，
        // 子进程仍会继续运行，直到它真正退出前都不能允许下一次写入。
        tokio::task::spawn_blocking(move || {
            let _tool_guards = tool_guards;
            let _execution_guard = execution_guard;
            operation(&tools)
        })
        .await
        .map_err(|e| format!("tool lifecycle task join error: {e}"))?
    }
}

static TOOL_LIFECYCLE: LazyLock<ToolLifecycleCoordinator> =
    LazyLock::new(ToolLifecycleCoordinator::default);

/// Display-only plan. Execution regenerates it under the lifecycle lock and rejects changes.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ToolUninstallPlan {
    tool: String,
    path: String,
    real_path: String,
    command: String,
    native: bool,
    wsl_distro: Option<String>,
}

/// One installation offered in the uninstall dialog: a plan, or why it must be removed manually.
#[derive(Debug, serde::Serialize)]
pub struct ToolUninstallCandidate {
    #[serde(flatten)]
    install: ToolInstallation,
    plan: Option<ToolUninstallPlan>,
    unavailable_reason: Option<String>,
    /// A copy the tool's own updater recreates (Grok in npm mode), so removing it alone is temporary.
    reinstalled_by_updater: bool,
}

#[tauri::command]
pub async fn prepare_tool_uninstall(
    tool: String,
    wsl_shell_by_tool: Option<HashMap<String, WslShellPreferenceInput>>,
) -> Result<Vec<ToolUninstallCandidate>, String> {
    if !VALID_TOOLS.contains(&tool.as_str()) {
        return Err(format!("Unsupported tool action target: {tool}"));
    }
    tokio::task::spawn_blocking(move || {
        let pref = wsl_shell_by_tool
            .as_ref()
            .and_then(|prefs| prefs.get(&tool));
        tool_uninstall::candidates(&tool, pref)
    })
    .await
    .map_err(|e| format!("uninstall plan task join error: {e}"))?
}

#[tauri::command]
pub async fn run_tool_lifecycle_action(
    tools: Vec<String>,
    action: String,
    wsl_shell_by_tool: Option<HashMap<String, WslShellPreferenceInput>>,
    uninstall_plan: Option<ToolUninstallPlan>,
) -> Result<(), String> {
    let requested = normalize_requested_tools(&tools);
    if requested.is_empty() {
        return Err("No supported tools selected".to_string());
    }

    if action == "uninstall" {
        let plan = uninstall_plan.ok_or("Preview the uninstall plan before confirming.")?;
        if requested.len() != 1 || requested[0] != plan.tool {
            return Err("The uninstall plan does not match the selected tool.".into());
        }
        return TOOL_LIFECYCLE
            .run(requested, move |tools| {
                for tool in tools {
                    let pref = wsl_shell_by_tool
                        .as_ref()
                        .and_then(|prefs| prefs.get(*tool));
                    tool_uninstall::uninstall(tool, pref, &plan)?;
                }
                Ok(())
            })
            .await;
    }
    let action = ToolLifecycleAction::from_str(&action)?;
    if matches!(action, ToolLifecycleAction::Update) {
        for tool in &requested {
            tool_node_requirement::ensure_update_node(tool).await?;
        }
    }

    let label = match action {
        ToolLifecycleAction::Install => "tool_install",
        ToolLifecycleAction::Update => "tool_update",
    };

    // 排队结束后再探测安装目标，与执行一起持锁，避免读到另一升级的中间状态。
    TOOL_LIFECYCLE
        .run(requested, move |tools| {
            let command_line =
                build_tool_lifecycle_command(tools, action, wsl_shell_by_tool.as_ref())?;
            run_tool_lifecycle_silently(&command_line, label)
        })
        .await
}

/// 静默执行工具安装/更新脚本：直接捕获子进程输出并阻塞到命令真正结束，
/// 不再弹出可见终端窗口（与 `launch_terminal_running` 的"开窗即返回"形成对比，
/// 后者仍保留给 provider 切换等需要交互式终端的场景）。
/// 失败时回传 stderr/stdout 末尾若干行，供前端 toast 提示。
#[cfg(not(target_os = "windows"))]
fn run_tool_lifecycle_silently(command_line: &str, _label: &str) -> Result<(), String> {
    use std::process::Command;
    // command_line 是 bash 风格脚本（含 `set -e` 与多行命令）；强制用 bash 执行，
    // 避免用户默认 shell 为 fish/zsh 时 `set -e` 等语义不一致。
    let mut cmd = Command::new("bash");
    cmd.arg("-c").arg(command_line);
    // GUI App 继承的是 launchd 的窄 PATH，而锚定探测用的是登录 shell —— 见
    // `login_shell_path` doc。命令自身用绝对路径不受影响，但工具**内部** spawn 的
    // npm/node 等子进程、以及 install 链里裸的 npm fallback 都需要真实 PATH。
    if let Some(login_path) = login_shell_path(None) {
        let inherited = std::env::var("PATH").unwrap_or_default();
        cmd.env("PATH", merge_path_segments(&login_path, &inherited));
    }
    let output = cmd.output().map_err(|e| format!("启动安装进程失败: {e}"))?;
    finish_lifecycle_output(&output)
}

/// Windows 静默执行：command_line 是 .bat 内容（chcp 65001 + @echo off + call/wsl 行，CRLF 分隔），
/// 写临时 .bat 后用 `cmd /C` 执行，`CREATE_NO_WINDOW` 抑制 console 窗口。
#[cfg(target_os = "windows")]
fn run_tool_lifecycle_silently(command_line: &str, label: &str) -> Result<(), String> {
    // 每次调用使用独立目录，避免同进程并发升级时覆盖或删除另一工具的脚本。
    let script_dir = tempfile::Builder::new()
        .prefix("cc_switch_lifecycle_")
        .tempdir()
        .map_err(|e| format!("创建批处理目录失败: {e}"))?;
    let bat_file = script_dir.path().join(format!("{label}.bat"));
    std::fs::write(&bat_file, command_line).map_err(|e| format!("写入批处理文件失败: {e}"))?;

    let output = windows_lifecycle_command(&bat_file, &effective_path_string()).output();

    finish_lifecycle_output(&output.map_err(|e| format!("启动安装进程失败: {e}"))?)
}

#[cfg(target_os = "windows")]
fn windows_lifecycle_command(bat_file: &Path, effective_path: &str) -> std::process::Command {
    let mut command = std::process::Command::new("cmd");
    command
        .arg("/C")
        .arg(bat_file)
        // MSI 自动重启可能丢失用户 PATH（#8171）。执行与探测共用补全后的 PATH，
        // 让裸 npm 兜底及工具内部启动的 node/npm 都能找到；仅设置子进程环境。
        .env("PATH", effective_path)
        .creation_flags(CREATE_NO_WINDOW);
    command
}

/// 把子进程退出结果转成 `Result`：成功返回 `Ok`；失败提取 stderr（空则回退 stdout）
/// 的末尾若干行作为错误详情，避免把整段安装日志塞进 toast。
fn finish_lifecycle_output(output: &std::process::Output) -> Result<(), String> {
    if output.status.success() {
        return Ok(());
    }
    let stderr = decode_command_output(&output.stderr);
    let stdout = decode_command_output(&output.stdout);
    let raw = if stderr.trim().is_empty() {
        stdout.trim()
    } else {
        stderr.trim()
    };
    let detail = npm_bin_collision_detail(raw).unwrap_or_else(|| last_lines(raw, 8));
    Err(if detail.is_empty() {
        format!("命令执行失败 (exit code: {:?})", output.status.code())
    } else {
        append_npm_bin_collision_hint(detail, raw)
    })
}

/// npm 全局安装在链接可执行文件时若目标路径已被其他安装方式留下的启动器
/// 占用，会以 EEXIST 失败，原始输出只建议 `--force` 覆盖（#9173）。识别这种
/// bin 路径冲突后在原始输出后追加可执行提示：指出冲突文件、给出安全的
/// 备份改名/改用原安装方式两条路，并明确提醒不要盲目 `--force`。
/// 非此类失败原样返回，不改变既有错误文案。
fn append_npm_bin_collision_hint(detail: String, raw: &str) -> String {
    let Some(path) = npm_bin_collision_path(raw) else {
        return detail;
    };
    format!(
        "{detail}\n\nHint: npm could not create the executable because {path} already exists. \
         If it is a launcher from another installation, back it up or rename it \
         (for example to {path}.bak) and try again, or update the tool with the \
         installation method that created it. Do not use npm --force unless you \
         mean to overwrite the existing file."
    )
}

/// 从 npm 输出里提取 bin 冲突路径：要求同时出现 `EEXIST` 与
/// `File exists: <path>` 行，避免把其他含 EEXIST 字样的失败误判为路径冲突。
fn npm_bin_collision_path(raw: &str) -> Option<&str> {
    if !raw.contains("EEXIST") {
        return None;
    }
    raw.lines().find_map(|line| {
        line.split_once("File exists:")
            .map(|(_, path)| path.trim())
            .filter(|path| !path.is_empty())
    })
}

/// npm 链接可执行文件撞上不归它管的同名文件时以 EEXIST 退出（#9173）。关键的两行
/// 可能被尾部的 `--force` 建议、更新通知和日志路径挤出最后 8 行，这里单独保留，
/// 前端据此换成可操作的提示。
fn npm_bin_collision_detail(text: &str) -> Option<String> {
    let code = text.lines().find(|line| line.contains("EEXIST"))?;
    let exists = text.lines().find(|line| line.contains("File exists:"))?;
    Some(format!("{}\n{}", code.trim(), exists.trim()))
}

/// 取文本末尾最多 `n` 行（npm / pip 的关键错误通常出现在输出尾部）。
fn last_lines(text: &str, n: usize) -> String {
    let lines: Vec<&str> = text.lines().collect();
    let start = lines.len().saturating_sub(n);
    lines[start..].join("\n")
}

pub(crate) fn decode_command_output(bytes: &[u8]) -> String {
    #[cfg(target_os = "windows")]
    {
        decode_windows_command_output(bytes)
    }

    #[cfg(not(target_os = "windows"))]
    {
        String::from_utf8_lossy(bytes).into_owned()
    }
}

#[cfg(target_os = "windows")]
fn decode_windows_command_output(bytes: &[u8]) -> String {
    if bytes.is_empty() {
        return String::new();
    }

    if let Ok(text) = std::str::from_utf8(bytes) {
        return text.to_string();
    }

    use windows_sys::Win32::Globalization::{GetACP, GetOEMCP, MultiByteToWideChar};

    fn decode_codepage(bytes: &[u8], codepage: u32) -> Option<String> {
        if codepage == 0 {
            return None;
        }

        let input_len = i32::try_from(bytes.len()).ok()?;
        unsafe {
            let wide_len = MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                input_len,
                std::ptr::null_mut(),
                0,
            );
            if wide_len <= 0 {
                return None;
            }

            let mut wide = vec![0u16; wide_len as usize];
            let written = MultiByteToWideChar(
                codepage,
                0,
                bytes.as_ptr(),
                input_len,
                wide.as_mut_ptr(),
                wide_len,
            );
            if written <= 0 {
                return None;
            }

            Some(String::from_utf16_lossy(&wide[..written as usize]))
        }
    }

    let oem_cp = unsafe { GetOEMCP() };
    if let Some(decoded) = decode_codepage(bytes, oem_cp) {
        return decoded;
    }

    let ansi_cp = unsafe { GetACP() };
    if ansi_cp != oem_cp {
        if let Some(decoded) = decode_codepage(bytes, ansi_cp) {
            return decoded;
        }
    }

    String::from_utf8_lossy(bytes).into_owned()
}





fn tool_display_name(tool: &str) -> &'static str {
    match tool {
        "claude" => "Claude Code",
        "codex" => "Codex",
        "gemini" => "Gemini CLI",
        "grok" => "Grok Build",
        "opencode" => "OpenCode",
        "openclaw" => "OpenClaw",
        "hermes" => "Hermes",
        "pi" => "Pi",
        "omp" => "Oh My Pi",
        "mcode" => "MiniMax Code",
        "kimi" => "Kimi Code",
        "dsh" => "DeepSeek Harness",
        "agy" => "Antigravity CLI",
        _ => "Unknown",
    }
}



// The npm distribution needs Node >=22.19. The native installer bundles its runtime.

// Oh My Pi 的 npm 包只能由 bun 运行（入口是 `#!/usr/bin/env bun` 的 TypeScript）。官方
// installer 有匹配架构的 bun 时走 bun，否则下载并校验预编译二进制到 `~/.local/bin`，
// 所以新装不要求本机先有 bun。




















fn npm_install_command_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some(
            "npm i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code",
        ),
        "codex" => Some("npm i -g @openai/codex@latest"),
        "gemini" => Some("npm i -g @google/gemini-cli@latest"),
        "grok" => Some(
            "npm i -g @xai-official/grok@latest --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok",
        ),
        "opencode" => Some(
            "npm i -g opencode-ai@latest --ignore-scripts=false --include=optional --allow-scripts=opencode-ai",
        ),
        "openclaw" => Some("npm i -g openclaw@latest"),
        "pi" => Some("npm i -g @earendil-works/pi-coding-agent@latest"),
        "kimi" => Some("npm i -g @moonshot-ai/kimi-code@latest"),
        "dsh" => Some("npm i -g @deepseek-ai/dsh@latest --engine-strict"),
        "mcode" => Some(
            "npm i -g @minimax-ai/code@latest --ignore-scripts=false --include=optional \"--allow-scripts=@minimax-ai/code,better-sqlite3\"",
        ),
        _ => None,
    }
}

/// `npm i -g` 时须追加的参数（前导空格已含），与 `npm_install_command_for` 的静态命令
/// 保持一致，供锚定到某处 npm 的升级命令复用。
///
/// Claude Code / OpenCode 的 postinstall 把平台 optional dependency 中的原生程序放到
/// bin/claude.exe / bin/opencode.exe。npm 12 拦截该脚本时会留下文本占位文件，
/// Windows 执行后报“与 Windows 版本不兼容”。
/// Grok Build 的 bin/grok 是 node launcher，postinstall 才把 optional dependency 里的
/// 平台二进制解到 `~/.grok/bin/grok-<version>` 并 relink `grok`；被拦后 launcher 只要见到
/// `~/.grok/bin/grok` 就直接运行它、不比对版本，于是 npm 升级 exit 0 却仍跑旧版。
/// MiniMax Code 依赖 better-sqlite3 的安装脚本：npm 12 默认拦截依赖的 install 脚本，
/// 只放行 `--allow-scripts` 列出的包（按注册表包名匹配），被拦后 SQLite 不可用；
/// `--ignore-scripts=false` / `--include=optional` 抵消用户 npmrc 里的相反设置。
/// npm 10/11 上这串参数无副作用（实测 exit 0、脚本照常执行）。整段 `--allow-scripts=…`
/// 加双引号：逗号在 PowerShell 里会被当成数组分隔符，bash/cmd 都会剥掉这层引号。
fn npm_install_extra_args(tool: &str) -> &'static str {
    match tool {
        "dsh" => " --engine-strict",
        "claude" => {
            " --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code"
        }
        "grok" => " --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok",
        "opencode" => " --ignore-scripts=false --include=optional --allow-scripts=opencode-ai",
        "mcode" => {
            " --ignore-scripts=false --include=optional \"--allow-scripts=@minimax-ai/code,better-sqlite3\""
        }
        _ => "",
    }
}

// `omp update` 自己识别 brew / mise / nix / bun / npm / 预编译二进制并照原渠道升级，
// 非 TTY 下不询问，失败退出码非 0；Nix 安装它只打印提示就 exit 0，见
// `anchored_command_from_paths` 的 Nix 门控。
// MiniMax Code 虽有 `mcode update`，却刻意不列在这里：它在 stdin/stdout 非 TTY 时只打印
// "No interactive confirmation is available" 就 exit 0、并不安装。静默 lifecycle 的
// `cmd.output()` 正是非 TTY，放进来会让 `mcode update || <fallback>` 的兜底永不触发。
fn official_update_args(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" | "codex" | "grok" | "hermes" | "agy" | "omp" => Some("update"),
        "openclaw" => Some("update --yes"),
        "opencode" => Some("upgrade"),
        "kimi" => Some("upgrade --yes"),
        _ => None,
    }
}








/// Windows 双引号包裹基础原语:无条件加引号 + 内部 `"` 转义为 `\"`。
/// `windows_cmd_double_quote_arg`(给 wsl.exe 传 bash 命令字符串用)与
/// `win_quote_path_for_batch`(给锚定路径用)都基于它,避免两份 quoter 各自演化、
/// 未来对同一路径产生不一致引用形态。镜像 POSIX 侧 `shell_single_quote` 与
/// `quote_path_if_spaced` 的"重量基础 + 轻量条件包装"两层结构。
#[cfg(target_os = "windows")]
fn win_double_quote(value: &str) -> String {
    format!("\"{}\"", value.replace('"', "\\\""))
}

#[cfg(target_os = "windows")]
fn windows_cmd_double_quote_arg(value: &str) -> String {
    win_double_quote(value)
}




/// 该工具在 npm 上的预发布通道 tag(靠前者优先)。仅当本地版本已**严格领先**
/// `latest` 时才会被补查 —— 让主动在抢先通道的用户(如走 Claude Code 的 `next`)
/// 看到与所在通道对齐的"最新版本",同时绝不把稳定通道用户暴露给预发布版。
/// 返回空切片表示该工具只看 `latest`、不补查。
///
/// 为何不通用覆盖所有工具:各家预发布 tag 命名互不统一(codex=alpha/beta/native、
/// gemini=nightly/preview、openclaw=alpha/beta),且 codex 的 beta/native 是
/// `0.1.x` 时间戳式版本、gemini 有误发的 `false` tag —— 这些脏值虽会被
/// `pick_latest_version` 的版本比较挡掉,但维护成本与误报风险不值当,故暂只为
/// Claude Code 启用。
fn npm_prerelease_tags(tool: &str) -> &'static [&'static str] {
    match tool {
        "claude" => &["next"],
        _ => &[],
    }
}




















/// WSL 未指定 shell 时依次尝试的 flag：交互式初始化（nvm 等写在 rc 里）优先。
/// 卸载探测（`tool_uninstall`）按同一顺序选 flag，否则两边可能命中不同的安装。
#[cfg(any(target_os = "windows", test))]
const WSL_AUTO_SHELL_FLAGS: [&str; 3] = ["-lic", "-lc", "-c"];

/// WSL 未指定 shell 时的版本检测命令（由外层 `sh -c` 执行）：在 `$SHELL` 里按
/// `WSL_AUTO_SHELL_FLAGS` 依次重试，或只用调用方已校验过的 `flag`。
#[cfg(any(target_os = "windows", test))]
fn wsl_auto_shell_command(payload: &str, flag: Option<&str>) -> String {
    let flags = flag.map_or(WSL_AUTO_SHELL_FLAGS.to_vec(), |flag| vec![flag]);
    let last = flags.len() - 1;
    flags
        .iter()
        .enumerate()
        .map(|(i, flag)| {
            // 只有最后一次尝试保留 stderr，供报错展示
            let quiet = if i < last { " 2>/dev/null" } else { "" };
            format!("\"${{SHELL:-sh}}\" {flag} '{payload}'{quiet}")
        })
        .collect::<Vec<_>>()
        .join(" || ")
}









/// 校验 WSL 发行版名称是否合法
/// WSL 发行版名称只允许字母、数字、连字符、下划线和点（如 `Ubuntu-22.04`）
#[cfg(target_os = "windows")]
fn is_valid_wsl_distro_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_' || c == '.')
}

/// Validate that the given shell name is one of the allowed shells.
fn is_valid_shell(shell: &str) -> bool {
    matches!(
        shell.rsplit('/').next().unwrap_or(shell),
        "sh" | "bash" | "zsh" | "fish" | "dash"
    )
}

/// Validate that the given shell flag is one of the allowed flags.
#[cfg(target_os = "windows")]
fn is_valid_shell_flag(flag: &str) -> bool {
    matches!(flag, "-c" | "-lc" | "-lic")
}

/// Return the default invocation flag for the given shell.
fn default_flag_for_shell(shell: &str) -> &'static str {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "dash" | "sh" => "-c",
        "fish" => "-lc",
        _ => "-lic",
    }
}

// 以下 shell 解析辅助函数仅被 macOS/Linux 的终端启动逻辑使用；Windows 非 test 编译下为死代码。
#[cfg_attr(windows, allow(dead_code))]
fn fallback_user_shell() -> &'static str {
    if cfg!(target_os = "macos") {
        "/bin/zsh"
    } else {
        "/bin/bash"
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn valid_user_shell_path(shell: &str) -> bool {
    if shell.is_empty()
        || !shell.starts_with('/')
        || !is_valid_shell(shell)
        || shell.chars().any(char::is_control)
    {
        return false;
    }

    let path = std::path::Path::new(shell);
    path.is_file() && is_executable_file(path)
}

#[cfg(unix)]
fn is_executable_file(path: &std::path::Path) -> bool {
    use std::os::unix::fs::PermissionsExt;

    path.metadata()
        .map(|metadata| metadata.permissions().mode() & 0o111 != 0)
        .unwrap_or(false)
}

#[cfg(not(unix))]
#[cfg_attr(windows, allow(dead_code))]
fn is_executable_file(path: &std::path::Path) -> bool {
    path.is_file()
}

/// 获取用户默认 shell 的完整路径；异常或被污染的 SHELL 回退到平台默认值。
#[cfg_attr(windows, allow(dead_code))]
fn get_user_shell() -> String {
    std::env::var("SHELL")
        .ok()
        .filter(|shell| valid_user_shell_path(shell))
        .unwrap_or_else(|| fallback_user_shell().to_string())
}

/// 构建 exec 行：引号保护 shell 路径，交还用户 shell 让其按默认规则加载 rc 配置。
#[cfg_attr(windows, allow(dead_code))]
fn build_exec_line(shell: &str, cwd: Option<&Path>) -> String {
    let quoted_shell = shell_single_quote(shell);

    match shell.rsplit('/').next().unwrap_or(shell) {
        "zsh" => cwd
            .map(|dir| {
                let command = format!(
                    "cd {} || exit 1; exec {} -i",
                    shell_single_quote(&dir.to_string_lossy()),
                    quoted_shell
                );
                format!("exec {} -lc {}", quoted_shell, shell_single_quote(&command))
            })
            .unwrap_or_else(|| format!("exec {quoted_shell} -l")),
        _ => format!("exec {quoted_shell}"),
    }
}

/// 构建 provider 命令行：通过用户 shell 的交互模式执行，确保 GUI 启动的终端也加载用户 PATH。
#[cfg_attr(windows, allow(dead_code))]
fn build_provider_command_line(shell: &str, launch_command: &str, cwd: Option<&Path>) -> String {
    let command = cwd
        .map(|dir| {
            format!(
                "cd {} && {}",
                user_shell_quote(shell, &dir.to_string_lossy()),
                launch_command
            )
        })
        .unwrap_or_else(|| launch_command.to_string());

    format!(
        "{} {} {}",
        shell_single_quote(shell),
        provider_command_flag_for_shell(shell),
        shell_single_quote(&command)
    )
}

#[cfg_attr(windows, allow(dead_code))]
fn provider_command_flag_for_shell(shell: &str) -> &'static str {
    match shell.rsplit('/').next().unwrap_or(shell) {
        "dash" | "sh" => "-c",
        "zsh" => "-lic",
        _ => "-ic",
    }
}

#[cfg_attr(windows, allow(dead_code))]
fn build_final_shell_cd_command(shell: &str, cwd: Option<&Path>) -> String {
    if matches!(shell.rsplit('/').next().unwrap_or(shell), "zsh") {
        return String::new();
    }

    cwd.map(|dir| {
        format!(
            "cd {} || exit 1\n",
            shell_single_quote(&dir.to_string_lossy())
        )
    })
    .unwrap_or_default()
}



















/// The "effective PATH" used for CLI detection and lifecycle commands. On Windows
/// the inherited process PATH can be incomplete — most notably after an in-app self-update,
/// where the MSI/WiX-auto-launched process inherits only the machine-level PATH
/// and drops the user-level PATH (see #6061). Any CLI installed in a user-PATH
/// location (winget Claude `%LOCALAPPDATA%\Programs\claude`, the standalone
/// Codex installer `%LOCALAPPDATA%\Programs\OpenAI\Codex\bin`, a custom npm
/// prefix `D:\npm-global`, …) then reads as "not installed".
///
/// Reconstruct the effective PATH by merging the process PATH with the machine
/// and user registry PATH values (`REG_EXPAND_SZ` expanded), so detection sees
/// the same installations a freshly logged-in shell would — regardless of how
/// the current process was launched. Process entries are kept first (a runtime
/// override wins); registry entries fill whatever the process is missing;
/// duplicates are removed.
///
/// See `env_checker::check_system_env` for the same set of registry keys; here
/// referenced variables also fall back to these keys if absent from the process.
///
/// fnm's default Node alias is appended last (see `fnm_default_alias_dir_candidates`),
/// so a Node that only exists in `fnm env` shells is still visible, while every
/// explicit PATH entry keeps precedence.
#[cfg(target_os = "windows")]
fn effective_path_string() -> String {
    use winreg::enums::{HKEY_CURRENT_USER, HKEY_LOCAL_MACHINE};
    use winreg::RegKey;

    let process = std::env::var_os("PATH")
        .map(|value| value.to_string_lossy().into_owned())
        .unwrap_or_default();
    let user = RegKey::predef(HKEY_CURRENT_USER)
        .open_subkey("Environment")
        .and_then(|k| k.get_value::<String, &str>("Path"))
        .map(|raw| expand_env_chars(&raw))
        .unwrap_or_default();
    let machine = RegKey::predef(HKEY_LOCAL_MACHINE)
        .open_subkey("SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment")
        .and_then(|k| k.get_value::<String, &str>("Path"))
        .map(|raw| expand_env_chars(&raw))
        .unwrap_or_default();
    let fnm = fnm_default_alias_dir_candidates(
        std::env::var_os("FNM_DIR"),
        &dirs::home_dir().unwrap_or_default(),
        dirs::data_dir(),
    )
    .into_iter()
    .filter(|dir| dir.is_dir())
    .map(|dir| dir.to_string_lossy().into_owned())
    .collect::<Vec<_>>()
    .join(";");
    merge_path_segments_win(&[&process, &user, &machine, &fnm])
}







/// Merge several Windows PATH strings (`;`-separated) in order, keeping only
/// the first occurrence of each segment (case-insensitive). Process segments
/// come first to respect runtime overrides, followed by user- and
/// machine-level registry segments.
#[cfg(target_os = "windows")]
fn merge_path_segments_win(parts: &[&str]) -> String {
    let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
    let mut merged: Vec<&str> = Vec::new();
    for part in parts {
        for seg in part.split(';') {
            let s = seg.trim();
            if s.is_empty() || !seen.insert(s.to_ascii_lowercase()) {
                continue;
            }
            merged.push(s);
        }
    }
    merged.join(";")
}



/// Convert a canonicalized Windows path back to the form accepted by shell
/// commands. `std::fs::canonicalize` prefixes local paths with `\\?\` (and UNC
/// paths with `\\?\UNC\`), but `cmd.exe` cannot `call` a batch file through
/// those verbatim paths and reports "The system cannot find the path
/// specified." Direct Win32 executable launches accept the prefix; batch
/// scripts do not.
#[cfg(target_os = "windows")]
fn windows_shell_compatible_path(path: &Path) -> std::path::PathBuf {
    let raw = path.to_string_lossy();
    if let Some(unc) = raw.strip_prefix(r"\\?\UNC\") {
        std::path::PathBuf::from(format!(r"\\{unc}"))
    } else if let Some(local) = raw.strip_prefix(r"\\?\") {
        std::path::PathBuf::from(local)
    } else {
        path.to_path_buf()
    }
}




























/// 工具对应的 npm 包名（hermes 走自己的 CLI/installer，不在此表）。锚定升级据此拼 `npm i -g`。
/// 全平台共用一张表——Windows 锚定层(`anchored_command_from_paths` 的 windows 版)也读这里。
fn npm_package_for(tool: &str) -> Option<&'static str> {
    match tool {
        "claude" => Some("@anthropic-ai/claude-code"),
        "codex" => Some("@openai/codex"),
        "gemini" => Some("@google/gemini-cli"),
        "grok" => Some("@xai-official/grok"),
        "opencode" => Some("opencode-ai"),
        "openclaw" => Some("openclaw"),
        "pi" => Some("@earendil-works/pi-coding-agent"),
        "mcode" => Some("@minimax-ai/code"),
        "kimi" => Some("@moonshot-ai/kimi-code"),
        "omp" => Some("@oh-my-pi/pi-coding-agent"),
        "dsh" => Some("@deepseek-ai/dsh"),
        _ => None,
    }
}
































































/// 一次"探测工具安装分布"的结果：枚举到的所有安装 + 各项衍生判定。同时服务两条
/// 路径——诊断展示（`is_conflict`）与升级确认（`needs_confirmation`/`command`/`anchored`）。
/// 字段保持 snake_case（与 `ToolInstallation` 一致），前端按同名读取。
#[derive(Debug, serde::Serialize)]
pub struct ToolInstallationReport {
    tool: String,
    /// 该工具枚举到的所有安装。
    installs: Vec<ToolInstallation>,
    /// 严阈值：≥2 且(版本分歧或运行态混合)。诊断按钮/自动补诊据此展示冲突。
    is_conflict: bool,
    /// 宽阈值：≥2 处。升级确认据此弹窗（升级只动一处，任何多处都该让用户知情）。
    needs_confirmation: bool,
    /// 锚定后将执行的升级命令（仅展示；真正执行时后端会重新生成，不信任前端回传）。
    command: String,
    /// 是否成功锚定到某处具体安装。false = 退到裸 fallback 命令（无法确定命令行实际
    /// 命中哪处，或该处无同级 npm）；前端据此给出"默认入口无法确定"的诚实文案。
    anchored: bool,
    /// 默认那处是认不出安装渠道的原生可执行文件（见 `UpdateCommand::Unmanaged`）：
    /// 不会执行升级，此时 `command` 为空；前端跳过该工具并提示用原安装方式升级。
    unmanaged: bool,
}

/// 探测各工具的安装分布：枚举所有安装、标记冲突、生成锚定升级命令。只读、无副作用。
/// 诊断按钮、升级前确认、升级后补诊共用此命令，各取所需字段——避免对同一份枚举结果
/// 散落多套下游判定。
#[tauri::command]
pub async fn probe_tool_installations(
    tools: Vec<String>,
) -> Result<Vec<ToolInstallationReport>, String> {
    let requested = normalize_requested_tools(&tools);
    if requested.is_empty() {
        return Err("No supported tools selected".to_string());
    }
    tokio::task::spawn_blocking(move || {
        requested
            .into_iter()
            .map(|tool| {
                let installs = enumerate_tool_installations(tool);
                let (update, needs_confirmation) = plan_command_for(tool, &installs);
                let (command, anchored, unmanaged) = match update {
                    UpdateCommand::Anchored(command) => (command, true, false),
                    UpdateCommand::Static(command) => (command, false, false),
                    UpdateCommand::Unmanaged => (String::new(), false, true),
                };
                let is_conflict = is_conflicting(&installs);
                ToolInstallationReport {
                    tool: tool.to_string(),
                    installs,
                    is_conflict,
                    needs_confirmation,
                    command,
                    anchored,
                    unmanaged,
                }
            })
            .collect()
    })
    .await
    .map_err(|e| format!("probe task join error: {e}"))
}

/// 「应用」页显示每个工具的路径、安装来源和多处安装。和升级前的预检是同一份枚举，
/// 单列一个命令只为把「打开页面时的展示」和「点升级时的预检」分开调用。
#[tauri::command]
pub async fn list_tool_installations(
    tools: Vec<String>,
) -> Result<Vec<ToolInstallationReport>, String> {
    probe_tool_installations(tools).await
}

/// 这个工具的配置目录是不是指向 WSL（只有 Windows 上会是）。
pub(crate) fn tool_runs_in_wsl(tool: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        wsl_distro_for_tool(tool).is_some()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tool;
        false
    }
}

#[cfg(target_os = "windows")]
fn wsl_distro_for_tool(tool: &str) -> Option<String> {
    let app = match tool {
        "claude" => AppType::Claude,
        "codex" => AppType::Codex,
        "gemini" => AppType::Gemini,
        "grok" => AppType::GrokBuild,
        "opencode" => AppType::OpenCode,
        "openclaw" => AppType::OpenClaw,
        "hermes" => AppType::Hermes,
        "pi" => AppType::Pi,
        "omp" => AppType::Omp,
        _ => return None,
    };
    let override_dir = crate::settings::get_override_dir(&app)?;

    wsl_distro_from_path(&override_dir)
}

/// `hermes dashboard` 的启动命令行（交给 `launch_terminal_running`）。
///
/// Windows 上 Hermes 配置目录指向 WSL（UNC 路径）时，要进对应发行版里启动，
/// 否则跑起来的是 Windows 本机的 hermes（或报找不到命令），#8689。
/// Web UI 探测的 `127.0.0.1` 端口由 WSL2 的 localhost 转发到发行版里，不用改。
pub(crate) fn hermes_dashboard_command_line() -> Result<String, String> {
    #[cfg(target_os = "windows")]
    {
        if let Some(distro) = wsl_distro_for_tool("hermes") {
            return wsl_hermes_dashboard_line(&distro);
        }
    }
    Ok("hermes dashboard".to_string())
}

/// 用发行版里用户自己的登录交互 shell（`$SHELL -lic`）跑，PATH 才带得上
/// `~/.local/bin` 这类在 `.bashrc` / `.profile` 里加的目录。只用一个 flag、
/// 不像版本探测那样失败后换 flag 重试：dashboard 被 Ctrl+C 结束也是非零退出，
/// 重试会把它再拉起来。
#[cfg(target_os = "windows")]
fn wsl_hermes_dashboard_line(distro: &str) -> Result<String, String> {
    if !is_valid_wsl_distro_name(distro) {
        return Err(format!("Invalid WSL distro name: {distro}"));
    }
    Ok(format!(
        "wsl.exe -d {distro} -- sh -c {}",
        windows_cmd_double_quote_arg(&wsl_auto_shell_command("hermes dashboard", Some("-lic")))
    ))
}

/// 从 UNC 路径中提取 WSL 发行版名称
/// 支持 `\\wsl$\Ubuntu\...` 和 `\\wsl.localhost\Ubuntu\...` 两种格式
#[cfg(target_os = "windows")]
fn wsl_distro_from_path(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};
    let Some(Component::Prefix(prefix)) = path.components().next() else {
        return None;
    };
    match prefix.kind() {
        Prefix::UNC(server, share) | Prefix::VerbatimUNC(server, share) => {
            let server_name = server.to_string_lossy();
            if server_name.eq_ignore_ascii_case("wsl$")
                || server_name.eq_ignore_ascii_case("wsl.localhost")
            {
                let distro = share.to_string_lossy().to_string();
                if !distro.is_empty() {
                    return Some(distro);
                }
            }
            None
        }
        _ => None,
    }
}

/// 打开指定提供商的终端
///
/// 根据提供商配置的环境变量启动一个带有该提供商特定设置的终端
/// 无需检查是否为当前激活的提供商，任何提供商都可以打开终端
#[allow(non_snake_case)]
#[tauri::command]
pub async fn open_provider_terminal(
    state: State<'_, crate::store::AppState>,
    agy_accounts: State<'_, crate::commands::AgyAccountState>,
    app: String,
    #[allow(non_snake_case)] providerId: String,
    cwd: Option<String>,
    #[allow(non_snake_case)] skipPermissions: Option<bool>,
    #[allow(non_snake_case)] extraArgs: Option<String>,
) -> Result<bool, String> {
    let app_type = AppType::from_str(&app).map_err(|e| e.to_string())?;
    let launch_cwd = resolve_launch_cwd(cwd)?;
    // 先拆参数：填错了直接报错，不必等到切完 agy 登录、写完临时文件。
    let extra_args = split_launch_args(extraArgs.as_deref().unwrap_or_default())?;

    // 获取提供商配置
    let providers = ProviderService::list(state.inner(), app_type.clone())
        .map_err(|e| format!("获取提供商列表失败: {e}"))?;

    let provider = providers
        .get(&providerId)
        .ok_or_else(|| format!("提供商 {providerId} 不存在"))?;

    let mut launch = match app_type {
        AppType::Agy => {
            switch_agy_to_bound_account(&agy_accounts.0, provider).await?;
            agy_terminal_launch(provider)?
        }
        AppType::Codex => codex_terminal_launch(state.db.as_ref(), provider)?,
        AppType::Claude => {
            use crate::services::provider::claude_official::{terminal_account, AccountTerminal};
            match terminal_account(provider).map_err(|e| e.to_string())? {
                Some(AccountTerminal::Dir(config_dir)) => {
                    crate::services::claude_account::sync_shared(&config_dir);
                    claude_account_terminal_launch(Some(config_dir))
                }
                Some(AccountTerminal::Default) => {
                    crate::services::provider::claude_official::ensure_default_terminal_is_official(
                        &state.db,
                    )
                    .map_err(|e| e.to_string())?;
                    claude_account_terminal_launch(None)
                }
                None => {
                    let env_vars =
                        extract_env_vars_from_config(&provider.settings_config, &app_type);
                    let drops = crate::services::provider::claude_switch_drops(
                        state.inner(),
                        &provider.settings_config,
                    )
                    .unwrap_or_else(|e| {
                        log::warn!("计算终端要挡住的 live 键失败，按原样启动: {e}");
                        Default::default()
                    });
                    claude_terminal_launch(&env_vars, &providerId, &provider.name, &drops)?
                }
            }
        }
        _ => {
            let env_vars = extract_env_vars_from_config(&provider.settings_config, &app_type);
            claude_terminal_launch(&env_vars, &providerId, &provider.name, &Default::default())?
        }
    };
    if skipPermissions == Some(true) {
        launch.skip_permissions();
    }
    if let Err(e) = launch.append_user_args(extra_args) {
        for file in &launch.cleanup {
            let _ = std::fs::remove_file(file);
        }
        return Err(e);
    }

    launch_provider_terminal(&launch, launch_cwd.as_deref())
        .map_err(|e| format!("启动终端失败: {e}"))?;

    Ok(true)
}

/// 从提供商配置中提取环境变量
fn extract_env_vars_from_config(
    config: &serde_json::Value,
    app_type: &AppType,
) -> Vec<(String, String)> {
    let mut env_vars = Vec::new();

    let Some(obj) = config.as_object() else {
        return env_vars;
    };

    // 处理 env 字段（Claude/Gemini 通用）
    if let Some(env) = obj.get("env").and_then(|v| v.as_object()) {
        for (key, value) in env {
            if let Some(str_val) = value.as_str() {
                env_vars.push((key.clone(), str_val.to_string()));
            }
        }

        // 处理 base_url: 根据应用类型添加对应的环境变量
        let base_url_key = match app_type {
            AppType::Claude | AppType::ClaudeDesktop => Some("ANTHROPIC_BASE_URL"),
            AppType::Gemini => Some("GOOGLE_GEMINI_BASE_URL"),
            _ => None,
        };

        if let Some(key) = base_url_key {
            if let Some(url_str) = env.get(key).and_then(|v| v.as_str()) {
                env_vars.push((key.to_string(), url_str.to_string()));
            }
        }
    }

    // Codex 使用 auth 字段转换为 OPENAI_API_KEY
    if *app_type == AppType::Codex {
        if let Some(auth) = obj.get("auth").and_then(|v| v.as_str()) {
            env_vars.push(("OPENAI_API_KEY".to_string(), auth.to_string()));
        }
    }

    // Gemini 使用 api_key 字段转换为 GEMINI_API_KEY
    if *app_type == AppType::Gemini {
        if let Some(api_key) = obj.get("api_key").and_then(|v| v.as_str()) {
            env_vars.push(("GEMINI_API_KEY".to_string(), api_key.to_string()));
        }
    }

    env_vars
}

fn resolve_launch_cwd(cwd: Option<String>) -> Result<Option<PathBuf>, String> {
    let Some(raw_path) = cwd.filter(|value| !value.trim().is_empty()) else {
        return Ok(None);
    };

    if raw_path.contains('\n') || raw_path.contains('\r') {
        return Err("目录路径包含非法换行符".to_string());
    }

    let path = Path::new(&raw_path);
    if !path.exists() {
        return Err(format!("目录不存在: {raw_path}"));
    }

    let resolved = std::fs::canonicalize(path).map_err(|e| format!("解析目录失败: {e}"))?;
    if !resolved.is_dir() {
        return Err(format!("选择的路径不是文件夹: {}", resolved.display()));
    }

    #[cfg(target_os = "windows")]
    let resolved = windows_shell_compatible_path(&resolved);

    Ok(Some(resolved))
}

/// 在终端里启动的一次供应商会话：跑什么命令、只给它哪些环境变量、退出后删哪些临时
/// 文件。打开终端那一层（各家终端的 AppleScript、Linux 候选列表、Windows start）对所有
/// 应用共用，只有这里按应用不同。
struct ProviderLaunch {
    /// 启动前打印的说明
    title: &'static str,
    /// 说明下面打印的路径（只在 macOS / Linux 打印）
    #[cfg_attr(windows, allow(dead_code))]
    detail: Option<PathBuf>,
    program: &'static str,
    /// macOS / Linux 的参数，交给用户 shell 执行前逐个加引号
    #[cfg_attr(windows, allow(dead_code))]
    args: Vec<String>,
    /// Windows .bat 里执行的命令
    #[cfg_attr(not(windows), allow(dead_code))]
    batch_command: String,
    /// 只给这一个进程的环境变量；`None` 表示要清掉（用户 rc 里可能 export 过同名变量）。
    /// macOS / Linux 写进 0600 的临时文件、读完立即删除；Windows 写进 .bat 的 `setlocal` 段。
    env: Vec<(&'static str, Option<String>)>,
    /// 只在 .bat 里设置、给 `batch_command` 引用的变量（比如可能含非 ASCII 的路径）
    #[cfg_attr(not(windows), allow(dead_code))]
    batch_vars: Vec<(&'static str, String)>,
    /// 退出时删除的临时文件，都在系统临时目录里
    cleanup: Vec<PathBuf>,
}

impl ProviderLaunch {
    /// 用户在「打开终端」里勾了跳过权限确认：只加给这一次启动，不写进任何配置。
    fn skip_permissions(&mut self) {
        // Claude Code 和 agy 的参数同名；Codex 跳过确认的同时也关掉沙箱。
        let flag = match self.program {
            "codex" => "--dangerously-bypass-approvals-and-sandbox",
            _ => "--dangerously-skip-permissions",
        };
        self.args.push(flag.to_string());
        self.batch_command.push(' ');
        self.batch_command.push_str(flag);
    }

    /// 用户在「打开终端」里填的附加参数，接在 CC Switch 生成的参数后面，同样只给这一次。
    /// Windows 上每个参数经 .bat 变量传、用引号包住，cmd 的 `&`、`|` 之类不会接出别的命令。
    fn append_user_args(&mut self, args: Vec<String>) -> Result<(), String> {
        // Codex 的启动行带 `call`：`%` 会再展开一遍、引号里的 `^` 会加倍，和 `-c` 值同一条限制。
        if cfg!(target_os = "windows") && args.iter().any(|arg| arg.contains(['"', '%', '^'])) {
            return Err("附加参数里有 Windows 终端传不过去的字符（\" % ^）".to_string());
        }
        for (arg, var) in args.into_iter().zip(USER_ARG_BATCH_VARS) {
            // 程序按 Windows 的规则拆命令行：收尾引号前的反斜杠要成对，否则 `C:\work\` 会把
            // 引号当成内容吞掉。
            let trailing = arg.len() - arg.trim_end_matches('\\').len();
            let quoted_value = format!("{arg}{}", "\\".repeat(trailing));
            self.batch_command.push_str(&format!(" \"%{var}%\""));
            self.batch_vars.push((var, quoted_value));
            self.args.push(arg);
        }
        Ok(())
    }
}

/// 附加参数在 .bat 里经这些变量传，也是附加参数的个数上限（各平台一样）。
const USER_ARG_BATCH_VARS: [&str; 16] = [
    "CC_SWITCH_ARG0",
    "CC_SWITCH_ARG1",
    "CC_SWITCH_ARG2",
    "CC_SWITCH_ARG3",
    "CC_SWITCH_ARG4",
    "CC_SWITCH_ARG5",
    "CC_SWITCH_ARG6",
    "CC_SWITCH_ARG7",
    "CC_SWITCH_ARG8",
    "CC_SWITCH_ARG9",
    "CC_SWITCH_ARG10",
    "CC_SWITCH_ARG11",
    "CC_SWITCH_ARG12",
    "CC_SWITCH_ARG13",
    "CC_SWITCH_ARG14",
    "CC_SWITCH_ARG15",
];

/// 把附加参数按空白拆开：单引号或双引号括起来的部分算同一个参数（引号本身去掉），没有
/// 反斜杠转义（Windows 路径里全是反斜杠）。只拆成参数，不经 shell，`;`、`&`、`|` 原样交给 CLI。
fn split_launch_args(input: &str) -> Result<Vec<String>, String> {
    let mut args = Vec::new();
    let mut current = String::new();
    let mut in_word = false;
    let mut quote: Option<char> = None;
    for c in input.chars() {
        if c.is_control() && !(quote.is_none() && c.is_whitespace()) {
            return Err("附加参数里不能有换行等控制字符".to_string());
        }
        match quote {
            Some(q) if c == q => quote = None,
            Some(_) => current.push(c),
            None if c == '"' || c == '\'' => {
                quote = Some(c);
                in_word = true;
            }
            None if c.is_whitespace() => {
                if in_word {
                    args.push(std::mem::take(&mut current));
                    in_word = false;
                }
            }
            None => {
                current.push(c);
                in_word = true;
            }
        }
    }
    if quote.is_some() {
        return Err("附加参数里的引号没有配对".to_string());
    }
    if in_word {
        args.push(current);
    }
    if args.len() > USER_ARG_BATCH_VARS.len() {
        return Err(format!("附加参数最多 {} 个", USER_ARG_BATCH_VARS.len()));
    }
    Ok(args)
}

/// Codex 的 `-c` 值在 .bat 里经这些变量传（值里可能有 `&` 之类 cmd 的特殊字符）。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const CODEX_OVERRIDE_BATCH_VARS: [&str; 24] = [
    "CC_SWITCH_CODEX_C0",
    "CC_SWITCH_CODEX_C1",
    "CC_SWITCH_CODEX_C2",
    "CC_SWITCH_CODEX_C3",
    "CC_SWITCH_CODEX_C4",
    "CC_SWITCH_CODEX_C5",
    "CC_SWITCH_CODEX_C6",
    "CC_SWITCH_CODEX_C7",
    "CC_SWITCH_CODEX_C8",
    "CC_SWITCH_CODEX_C9",
    "CC_SWITCH_CODEX_C10",
    "CC_SWITCH_CODEX_C11",
    "CC_SWITCH_CODEX_C12",
    "CC_SWITCH_CODEX_C13",
    "CC_SWITCH_CODEX_C14",
    "CC_SWITCH_CODEX_C15",
    "CC_SWITCH_CODEX_C16",
    "CC_SWITCH_CODEX_C17",
    "CC_SWITCH_CODEX_C18",
    "CC_SWITCH_CODEX_C19",
    "CC_SWITCH_CODEX_C20",
    "CC_SWITCH_CODEX_C21",
    "CC_SWITCH_CODEX_C22",
    "CC_SWITCH_CODEX_C23",
];

/// Codex：卡就是当前直连的那家时直接跑 `codex`；否则用 `-c` 只对这个进程换成它，Key 经
/// 环境变量传（见 `codex_terminal`），不改 `config.toml`。
fn codex_terminal_launch(
    db: &crate::database::Database,
    provider: &crate::provider::Provider,
) -> Result<ProviderLaunch, String> {
    use crate::services::provider::codex_terminal;
    let plan = codex_terminal::terminal_launch(db, provider).map_err(|e| e.to_string())?;
    let mut overrides = plan.overrides;
    let mut cleanup = Vec::new();
    if let Some(catalog) = plan.catalog {
        let path = unique_temp_path("codex_catalog", "json");
        std::fs::write(&path, catalog).map_err(|e| format!("写入临时模型目录失败: {e}"))?;
        overrides.push(codex_terminal::catalog_override(&path.to_string_lossy()));
        cleanup.push(path);
    }
    codex_launch(&provider.name, overrides, plan.env, cleanup.clone()).inspect_err(|_| {
        for file in &cleanup {
            let _ = std::fs::remove_file(file);
        }
    })
}

fn codex_launch(
    name: &str,
    overrides: Vec<String>,
    env: Vec<(&'static str, Option<String>)>,
    cleanup: Vec<PathBuf>,
) -> Result<ProviderLaunch, String> {
    if overrides.len() > CODEX_OVERRIDE_BATCH_VARS.len() {
        return Err(format!("「{name}」要覆盖的 Codex 配置项太多"));
    }
    // cmd 不认 `\"`，值里有双引号就会把后面的 `&` 等字符露在引号外面；`call` 会把 `%` 再展开
    // 一遍、把引号里的 `^` 加倍。
    if cfg!(target_os = "windows")
        && overrides
            .iter()
            .any(|value| value.contains(['"', '%', '^']))
    {
        return Err(format!(
            "「{name}」的 Codex 配置里有 Windows 终端传不过去的字符（\" % ^），请先切换到这张卡再打开终端"
        ));
    }

    let mut args = Vec::new();
    // npm 装的是 codex.cmd：不加 `call` 跑完不回到启动 .bat，后面删临时文件（含 Key 的
    // .bat 本身）的几行就不执行了。
    let mut batch_command = String::from("call codex");
    let mut batch_vars = Vec::new();
    for (value, var) in overrides.into_iter().zip(CODEX_OVERRIDE_BATCH_VARS) {
        args.push("-c".to_string());
        args.push(value.clone());
        batch_command.push_str(&format!(" -c \"%{var}%\""));
        batch_vars.push((var, value));
    }
    Ok(ProviderLaunch {
        title: if args.is_empty() {
            "Starting Codex with its current config"
        } else {
            "Using this provider for Codex in this terminal only (config.toml is unchanged)"
        },
        detail: None,
        program: "codex",
        args,
        batch_command,
        env,
        batch_vars,
        cleanup,
    })
}

/// Claude Code：把卡上的 env 和 `/model` 菜单写进临时 settings 文件，
/// `claude --settings <文件>` 启动。`drops` 是切到这张卡时会从 live 清掉的键，在临时文件里
/// 挡住（见 [`write_claude_config`]）。
fn claude_terminal_launch(
    env_vars: &[(String, String)],
    provider_id: &str,
    provider_name: &str,
    drops: &crate::services::provider::SwitchDrops,
) -> Result<ProviderLaunch, String> {
    let config_file = unique_temp_path(&format!("claude_{provider_id}"), "json");
    write_claude_config(&config_file, env_vars, provider_name, drops)?;
    let config_name = config_file
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_default();
    Ok(ProviderLaunch {
        title: "Using provider-specific claude config:",
        detail: Some(config_file.clone()),
        program: "claude",
        args: vec![
            "--settings".to_string(),
            config_file.to_string_lossy().into_owned(),
        ],
        // 配置文件和 .bat 同在 temp_dir（%TEMP% 含用户名），用 %~dp0 引用：目录由 cmd
        // 运行时展开，不进 .bat 字节，非 ASCII 用户名下也不会乱码（#8034）。
        batch_command: format!(
            "claude --settings \"%~dp0{}\"",
            escape_windows_batch_value(&config_name)
        ),
        env: Vec::new(),
        batch_vars: Vec::new(),
        cleanup: vec![config_file],
    })
}

/// Claude 官方卡绑了账号：`CLAUDE_CONFIG_DIR` 放在启动环境里（不用 `--settings`）。
/// 变量在进程启动前就设好，Claude Code 一开始预取钥匙串读的就是这个账号的条目
/// （写在 settings 里的话 macOS 上前 30 秒会用默认登录的令牌）。会把它指向别处的变量一并清掉。
/// 绑号卡的终端：清掉会改走别家服务或别的登录的变量，再指定账号目录；`None` 是账号正在默认
/// 槽位里，去掉 `CLAUDE_CONFIG_DIR` 用默认目录。
fn claude_account_terminal_launch(config_dir: Option<PathBuf>) -> ProviderLaunch {
    let mut env: Vec<(&'static str, Option<String>)> =
        crate::services::claude_account::login::CLAUDE_ACCOUNT_UNSET
            .iter()
            .map(|name| (*name, None))
            .collect();
    env.push((
        "CLAUDE_CONFIG_DIR",
        config_dir
            .as_ref()
            .map(|dir| dir.to_string_lossy().into_owned()),
    ));
    ProviderLaunch {
        title: if config_dir.is_some() {
            "Using this Claude account's config directory:"
        } else {
            "Using this Claude account via Claude Code's current sign-in."
        },
        detail: config_dir,
        program: "claude",
        args: Vec::new(),
        batch_command: "claude".to_string(),
        env,
        batch_vars: Vec::new(),
        cleanup: Vec::new(),
    }
}

/// 官方卡绑了授权中心的账号时，先把 agy 的全局登录切到它（已经是它就不写）。没绑的卡跟着
/// agy 当前的登录走。不能切号的平台（目前除 macOS 外）忽略绑定，照常打开。
async fn switch_agy_to_bound_account(
    accounts: &crate::services::agy_account::AgyAccountManager,
    provider: &crate::provider::Provider,
) -> Result<(), String> {
    if !crate::services::provider::agy_direct::is_official(provider) {
        return Ok(());
    }
    let Some(account_id) = provider
        .meta
        .as_ref()
        .and_then(|meta| meta.managed_account_id_for("agy_oauth"))
        .filter(|id| !id.trim().is_empty())
    else {
        return Ok(());
    };
    if !crate::services::agy_account::keychain::supported() {
        log::info!("[AgyAccount] 这个平台不能切号，忽略卡片绑定的账号");
        return Ok(());
    }
    if !accounts
        .list()
        .await
        .iter()
        .any(|account| account.id == account_id)
    {
        return Err(crate::error::AppError::localized(
            "agy.account.bound_missing",
            format!(
                "「{}」绑定的 Antigravity 账号不在这台电脑的授权中心里：请先在授权中心添加它，或编辑这张卡改选账号",
                provider.name
            ),
            format!(
                "The Antigravity account bound to \"{}\" is not in this computer's Auth Center. Add it there, or edit the card to choose another account.",
                provider.name
            ),
        )
        .to_string());
    }
    accounts
        .switch_to(&account_id)
        .await
        .map_err(|error| error.to_string())
}

/// Antigravity CLI：官方登录卡跑 `agy`，卡上设了模型就加 `--model`（只对这次会话生效）；
/// Key 卡用它的私有 `--gemini_dir`，Key 和地址只经环境变量传给这一个进程。
fn agy_terminal_launch(provider: &crate::provider::Provider) -> Result<ProviderLaunch, String> {
    let launch = crate::services::provider::agy_direct::terminal_launch(provider)
        .map_err(|e| e.to_string())?;
    let Some(dir) = launch.gemini_dir else {
        let (args, batch_command, batch_vars) = match launch.model {
            Some(model) => (
                vec![format!("--model={model}")],
                format!("agy \"--model=%{AGY_MODEL_BATCH_VAR}%\""),
                vec![(AGY_MODEL_BATCH_VAR, model)],
            ),
            None => (Vec::new(), "agy".to_string(), Vec::new()),
        };
        return Ok(ProviderLaunch {
            title: "Starting Antigravity CLI with your signed-in account",
            detail: None,
            program: "agy",
            args,
            batch_command,
            env: launch.env,
            batch_vars,
            cleanup: Vec::new(),
        });
    };
    Ok(ProviderLaunch {
        title: "Using a private Antigravity profile for this provider:",
        detail: Some(dir.clone()),
        program: "agy",
        args: vec![format!("--gemini_dir={}", dir.to_string_lossy())],
        batch_command: format!("agy \"--gemini_dir=%{AGY_DIR_BATCH_VAR}%\""),
        env: launch.env,
        batch_vars: vec![(AGY_DIR_BATCH_VAR, dir.to_string_lossy().into_owned())],
        cleanup: Vec::new(),
    })
}

/// 恢复 Antigravity 第三方 Key 模式的会话：和「打开终端」同样带私有目录和 Key，再接上
/// `--conversation`。卡上已经没有 Key 时私有目录对不上，直接报错，不退回官方登录。
pub(crate) fn open_agy_resume_terminal(
    provider: &crate::provider::Provider,
    conversation: &str,
    cwd: Option<String>,
) -> Result<(), String> {
    let mut launch = agy_terminal_launch(provider)?;
    // 只有 Key 卡有私有目录（启动前打印的那行路径）；官方卡的参数可能只有 `--model`。
    if launch.detail.is_none() {
        return Err(format!(
            "「{}」已不再带 API Key，无法恢复它在私有目录里的会话",
            provider.name
        ));
    }
    let resume = format!("--conversation={conversation}");
    launch.args.push(resume.clone());
    launch.batch_command = format!("{} {resume}", launch.batch_command);
    let cwd = resolve_launch_cwd(cwd)?;
    launch_provider_terminal(&launch, cwd.as_deref()).map_err(|e| format!("启动终端失败: {e}"))
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const AGY_DIR_BATCH_VAR: &str = "CC_SWITCH_AGY_DIR";
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const AGY_MODEL_BATCH_VAR: &str = "CC_SWITCH_AGY_MODEL";

/// macOS / Linux 的启动脚本里，环境变量先以这个前缀的名字读进来，再由 `env` 改名交给
/// 程序：用户 shell 的 rc 文件在中间执行，直接用原名会被 rc 里的 export 覆盖。
#[cfg_attr(windows, allow(dead_code))]
const LAUNCH_ENV_PREFIX: &str = "CC_SWITCH_LAUNCH_ENV_";

/// 同一进程里连开几个终端也不会撞名。
fn unique_temp_path(stem: &str, ext: &str) -> PathBuf {
    use std::sync::atomic::{AtomicU64, Ordering};
    static SEQ: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or_default();
    std::env::temp_dir().join(format!(
        "{stem}_{}_{nanos:x}{}.{ext}",
        std::process::id(),
        SEQ.fetch_add(1, Ordering::Relaxed)
    ))
}

/// 根据平台打开终端运行 `launch`。
fn launch_provider_terminal(launch: &ProviderLaunch, cwd: Option<&Path>) -> Result<(), String> {
    #[cfg(any(target_os = "macos", target_os = "linux"))]
    {
        let (script_file, env_file) = write_unix_launcher(launch, cwd)?;
        #[cfg(target_os = "macos")]
        let result = launch_macos_terminal(&script_file);
        #[cfg(target_os = "linux")]
        let result = launch_linux_terminal(&script_file);
        if result.is_err() {
            for file in launch.cleanup.iter().chain([&script_file, &env_file]) {
                let _ = std::fs::remove_file(file);
            }
        }
        result
    }

    #[cfg(target_os = "windows")]
    {
        let result = launch_windows_terminal(launch, cwd);
        if result.is_err() {
            for file in &launch.cleanup {
                let _ = std::fs::remove_file(file);
            }
        }
        result
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (launch, cwd);
        Err("不支持的操作系统".to_string())
    }
}

/// shell 里的一个词：只含安全字符时原样写，否则加单引号。
#[cfg_attr(windows, allow(dead_code))]
fn shell_word(value: &str) -> String {
    let plain = !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.' | '/' | ':' | '='));
    if plain {
        value.to_string()
    } else {
        shell_single_quote(value)
    }
}

/// 交给用户 shell 执行的启动命令。有环境变量时经 `env` 设置或清除，值从启动脚本读进来的
/// `CC_SWITCH_LAUNCH_ENV_<n>` 取，不出现在命令行里。
#[cfg_attr(windows, allow(dead_code))]
#[cfg(test)]
fn unix_launch_command(launch: &ProviderLaunch) -> String {
    unix_launch_command_in(launch, "/bin/sh")
}

/// 同上，参数按用户 shell 的规则加引号：这条命令由用户 shell 解析（见
/// `build_provider_command_line`），附加参数又是用户随手填的文本。
#[cfg_attr(windows, allow(dead_code))]
fn unix_launch_command_in(launch: &ProviderLaunch, shell: &str) -> String {
    let mut words = Vec::new();
    if !launch.env.is_empty() {
        words.push("env".to_string());
        for (index, (name, value)) in launch.env.iter().enumerate() {
            // 中转用的变量也不留给程序。
            words.push(format!("-u {LAUNCH_ENV_PREFIX}{index}"));
            if value.is_none() {
                words.push(format!("-u {name}"));
            }
        }
        for (index, (name, value)) in launch.env.iter().enumerate() {
            if value.is_some() {
                words.push(format!("{name}=\"${LAUNCH_ENV_PREFIX}{index}\""));
            }
        }
    }
    words.push(launch.program.to_string());
    words.extend(launch.args.iter().map(|arg| user_shell_word(shell, arg)));
    words.join(" ")
}

/// 0600 环境变量文件的内容，启动脚本 `.` 进来后立即删除。
#[cfg_attr(windows, allow(dead_code))]
fn unix_launch_env_file(launch: &ProviderLaunch) -> String {
    launch
        .env
        .iter()
        .enumerate()
        .filter_map(|(index, (_, value))| {
            value
                .as_ref()
                .map(|value| format!("{LAUNCH_ENV_PREFIX}{index}={}\n", shell_single_quote(value)))
        })
        .collect()
}

/// 启动脚本：读进环境变量文件并删掉它，跑启动命令，删掉临时文件，最后换成用户的交互 shell。
#[cfg_attr(windows, allow(dead_code))]
fn build_unix_launcher_script(
    launch: &ProviderLaunch,
    shell: &str,
    cwd: Option<&Path>,
    script_file: &Path,
    env_file: Option<&Path>,
) -> String {
    let mut temp_files: Vec<String> = launch
        .cleanup
        .iter()
        .map(PathBuf::as_path)
        .chain(env_file)
        .map(|file| shell_single_quote(&file.to_string_lossy()))
        .collect();
    temp_files.push(shell_single_quote(&script_file.to_string_lossy()));
    let remove_temp_files = format!("rm -f -- {}", temp_files.join(" "));

    let mut script = String::from("#!/usr/bin/env sh\n");
    script.push_str(&format!(
        "trap {} EXIT\n",
        shell_single_quote(&remove_temp_files)
    ));
    if let Some(env_file) = env_file {
        let quoted = shell_single_quote(&env_file.to_string_lossy());
        script.push_str(&format!("set -a\n. {quoted}\nset +a\nrm -f -- {quoted}\n"));
    }
    script.push_str(&format!("echo {}\n", shell_single_quote(launch.title)));
    if let Some(detail) = &launch.detail {
        script.push_str(&format!(
            "echo {}\n",
            shell_single_quote(&detail.to_string_lossy())
        ));
    }
    script.push_str(&build_provider_command_line(
        shell,
        &unix_launch_command_in(launch, shell),
        cwd,
    ));
    script.push('\n');
    // exec 不会触发 EXIT trap，所以在换成交互 shell 之前自己删。
    script.push_str(&remove_temp_files);
    script.push('\n');
    let launch_vars: Vec<String> = (0..launch.env.len())
        .map(|index| format!("{LAUNCH_ENV_PREFIX}{index}"))
        .collect();
    if !launch_vars.is_empty() {
        script.push_str(&format!("unset {}\n", launch_vars.join(" ")));
    }
    script.push_str(&build_final_shell_cd_command(shell, cwd));
    script.push_str(&build_exec_line(shell, cwd));
    script.push('\n');
    script
}

/// 写好启动脚本（和需要时的 0600 环境变量文件），返回两者路径。
#[cfg(any(target_os = "macos", target_os = "linux"))]
fn write_unix_launcher(
    launch: &ProviderLaunch,
    cwd: Option<&Path>,
) -> Result<(PathBuf, PathBuf), String> {
    use std::os::unix::fs::PermissionsExt;

    let shell = get_user_shell();
    let script_file = unique_temp_path("cc_switch_launcher", "sh");
    let env_file = unique_temp_path("cc_switch_launch_env", "sh");
    let env_content = unix_launch_env_file(launch);
    let has_env = !env_content.is_empty();
    if has_env {
        crate::config::atomic_write_private(&env_file, env_content.as_bytes())
            .map_err(|e| format!("写入启动环境失败: {e}"))?;
    }
    let script_content = build_unix_launcher_script(
        launch,
        &shell,
        cwd,
        &script_file,
        has_env.then_some(env_file.as_path()),
    );
    std::fs::write(&script_file, &script_content).map_err(|e| format!("写入启动脚本失败: {e}"))?;
    std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o700))
        .map_err(|e| format!("设置脚本权限失败: {e}"))?;
    Ok((script_file, env_file))
}

/// 写入 claude 配置文件（含凭据，0600）
///
/// 这个文件叠在用户级 `settings.json` 上：`env` 按键合并、顶层按键覆盖。所以：
/// - `modelPicker` 一定要写：它按层级整个胜出，不写的话路由 / 聚合模式的菜单会漏进这个
///   终端，选了就把别家的模型发给这张卡；
/// - 切换会清掉、卡上又没有的键（`drops`）写成空值挡住：Claude Code 把空串当没设（2.1.314
///   实测：代理的占位凭据不再随请求发出，没配的档位回到内置模型）。
fn write_claude_config(
    config_file: &std::path::Path,
    env_vars: &[(String, String)],
    provider_name: &str,
    drops: &crate::services::provider::SwitchDrops,
) -> Result<(), String> {
    use serde_json::Value;
    let mut config_obj = serde_json::Map::new();
    let mut env_obj = serde_json::Map::new();

    for (key, value) in env_vars {
        env_obj.insert(key.clone(), Value::String(value.clone()));
    }

    let model_picker = crate::live::project::claude::terminal_model_picker(&env_obj, provider_name);
    for key in drops.env.keys() {
        env_obj
            .entry(key.clone())
            .or_insert_with(|| Value::String(String::new()));
    }
    for (key, value) in &drops.top {
        let empty = match value {
            Value::String(_) => Value::String(String::new()),
            Value::Object(_) => Value::Object(serde_json::Map::new()),
            Value::Array(_) => Value::Array(Vec::new()),
            _ => continue,
        };
        config_obj.insert(key.clone(), empty);
    }
    config_obj.insert("env".to_string(), Value::Object(env_obj));
    config_obj.insert("modelPicker".to_string(), model_picker);

    let config_json =
        serde_json::to_string_pretty(&config_obj).map_err(|e| format!("序列化配置失败: {e}"))?;

    crate::config::atomic_write_private(config_file, config_json.as_bytes())
        .map_err(|e| format!("写入配置文件失败: {e}"))
}

/// macOS: 根据用户首选终端启动
#[cfg(target_os = "macos")]
fn launch_macos_terminal(script_file: &std::path::Path) -> Result<(), String> {
    let preferred = crate::settings::get_preferred_terminal();
    let terminal = preferred.as_deref().unwrap_or("terminal");
    let script_file = script_file.to_path_buf();

    // Try the preferred terminal first, fall back to Terminal.app if it fails
    // Note: Kitty doesn't need the -e flag, others do
    let result = match terminal {
        "iterm2" => launch_macos_iterm2(&script_file),
        "warp" => launch_macos_warp(&script_file),
        "alacritty" => launch_macos_open_app("Alacritty", &script_file, true),
        "kitty" => launch_macos_open_app("kitty", &script_file, false),
        "ghostty" => launch_macos_ghostty(&script_file),
        "otty" => launch_macos_otty(&script_file),
        "wezterm" => launch_macos_open_app("WezTerm", &script_file, true),
        "kaku" => launch_macos_open_app("Kaku", &script_file, true),
        _ => launch_macos_terminal_app(&script_file),
    };

    // If preferred terminal fails and it's not the default, try Terminal.app as fallback
    if result.is_err() && terminal != "terminal" {
        log::warn!(
            "首选终端 {} 启动失败，回退到 Terminal.app: {:?}",
            terminal,
            result.as_ref().err()
        );
        return launch_macos_terminal_app(&script_file);
    }

    result
}

/// Escape a value as an AppleScript string literal.
#[cfg(target_os = "macos")]
fn applescript_string_literal(value: &str) -> String {
    format!("\"{}\"", value.replace('\\', "\\\\").replace('"', "\\\""))
}

/// Build the launcher command literal used by AppleScript.
#[cfg(target_os = "macos")]
fn applescript_launcher_command(script_file: &std::path::Path) -> String {
    applescript_string_literal(&format!(
        "sh {}",
        shell_single_quote(&script_file.to_string_lossy())
    ))
}

/// Build a launcher command that replaces the terminal-created shell session.
#[cfg(target_os = "macos")]
fn applescript_exec_launcher_command(script_file: &std::path::Path) -> String {
    applescript_string_literal(&format!(
        "exec sh {}",
        shell_single_quote(&script_file.to_string_lossy())
    ))
}

/// macOS: Terminal.app AppleScript.
/// A cold `activate` creates a default empty window before `do script` opens the command session.
/// Use `launch` for cold starts so `do script` can create the only new session without reusing restored windows.
#[cfg(target_os = "macos")]
fn build_macos_terminal_applescript(script_file: &std::path::Path) -> String {
    format!(
        r#"set launcher_script to {launcher}
set was_running to application "Terminal" is running
tell application "Terminal"
    if was_running then
        activate
        do script launcher_script
    else
        launch
        do script launcher_script
        activate
    end if
end tell"#,
        launcher = applescript_exec_launcher_command(script_file)
    )
}

/// Run AppleScript through `osascript -e` with shared error handling.
#[cfg(target_os = "macos")]
fn run_terminal_osascript(applescript: &str, terminal_label: &str) -> Result<(), String> {
    use std::process::Command;

    let output = Command::new("osascript")
        .arg("-e")
        .arg(applescript)
        .output()
        .map_err(|e| format!("执行 osascript 失败: {e}"))?;

    if !output.status.success() {
        let stderr = decode_command_output(&output.stderr);
        return Err(format!(
            "{terminal_label} 执行失败 (exit code: {:?}): {}",
            output.status.code(),
            stderr
        ));
    }

    Ok(())
}

/// macOS: Terminal.app
#[cfg(target_os = "macos")]
fn launch_macos_terminal_app(script_file: &std::path::Path) -> Result<(), String> {
    run_terminal_osascript(
        &build_macos_terminal_applescript(script_file),
        "Terminal.app",
    )
}

#[cfg(target_os = "macos")]
fn launch_macos_otty(script_file: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    let otty_cli = find_macos_otty_cli().ok_or_else(|| {
        "未找到 Otty CLI。请将 Otty 安装到 /Applications 或 ~/Applications。".to_string()
    })?;

    let command = build_macos_dash_c_command(script_file);
    let tab_result = Command::new(&otty_cli)
        .args(["tab", "new", "--window", "0", "--command", &command])
        .output()
        .map_err(|e| format!("启动 Otty CLI 失败: {e}"))?;

    if tab_result.status.success() {
        return Ok(());
    }

    log::debug!(
        "Otty 新建 Tab 失败，改为新建窗口: {}",
        decode_command_output(&tab_result.stderr)
    );

    let window_result = Command::new(&otty_cli)
        .args(["open", "--command", &command])
        .output()
        .map_err(|e| format!("启动 Otty CLI 失败: {e}"))?;

    if window_result.status.success() {
        Ok(())
    } else {
        Err(format!(
            "Otty 新建窗口失败 (exit code: {:?}): {}",
            window_result.status.code(),
            decode_command_output(&window_result.stderr)
        ))
    }
}

#[cfg(target_os = "macos")]
fn find_macos_otty_cli() -> Option<std::path::PathBuf> {
    macos_otty_cli_candidates()
        .into_iter()
        .find(|path| path.is_file() && is_executable_file(path))
}

#[cfg(target_os = "macos")]
fn macos_otty_cli_candidates() -> Vec<std::path::PathBuf> {
    let mut candidates = vec![std::path::PathBuf::from(
        "/Applications/Otty.app/Contents/MacOS/otty-cli",
    )];

    if let Some(home) = std::env::var_os("HOME") {
        candidates.push(
            std::path::PathBuf::from(home).join("Applications/Otty.app/Contents/MacOS/otty-cli"),
        );
    }

    candidates.push(std::path::PathBuf::from("/usr/local/bin/otty"));
    candidates.push(std::path::PathBuf::from("/opt/homebrew/bin/otty"));

    if let Some(path) = std::env::var_os("PATH") {
        for directory in std::env::split_paths(&path) {
            candidates.push(directory.join("otty"));
            candidates.push(directory.join("otty-cli"));
        }
    }

    candidates
}

/// macOS: iTerm2
#[cfg(target_os = "macos")]
fn build_macos_iterm2_applescript(script_file: &std::path::Path) -> String {
    format!(
        r#"set launcher_script to {launcher}
set was_running to application "iTerm" is running
tell application "iTerm"
    if was_running then
        activate
        if (count of windows) = 0 then
            create window with default profile
        else
            tell current window
                create tab with default profile
            end tell
        end if
    else
        activate
        set waited to 0
        repeat while (count of windows) = 0
            delay 0.1
            set waited to waited + 1
            if waited >= 30 then exit repeat
        end repeat
        if (count of windows) = 0 then
            create window with default profile
        end if
    end if
    tell current session of current window
        write text launcher_script
    end tell
end tell"#,
        launcher = applescript_exec_launcher_command(script_file)
    )
}

/// macOS: iTerm2
#[cfg(target_os = "macos")]
fn launch_macos_iterm2(script_file: &std::path::Path) -> Result<(), String> {
    run_terminal_osascript(&build_macos_iterm2_applescript(script_file), "iTerm2")
}

/// Keep the launcher path inside a `sh -c` string.
/// A bare `.sh` passed through `open --args` may also be opened as a document.
#[cfg(target_os = "macos")]
fn build_macos_dash_c_command(script_file: &std::path::Path) -> String {
    format!(
        "exec sh {}",
        shell_single_quote(&script_file.to_string_lossy())
    )
}

/// macOS: Ghostty.
/// Warm starts use AppleScript to create one command window.
/// Cold starts use `initial-command` so the first default surface runs the launcher.
/// Do not use `initial-window=false` plus `new window`: cold launch can still create the default window first.
#[cfg(target_os = "macos")]
fn build_macos_ghostty_applescript(script_file: &std::path::Path) -> String {
    format!(
        r#"set launcher_command to {launcher}
set was_running to application "Ghostty" is running
if was_running then
    tell application "Ghostty"
        new window with configuration {{command:launcher_command}}
    end tell
else
    do shell script "open -na Ghostty --args --quit-after-last-window-closed=true " & quoted form of ("--initial-command=" & launcher_command)
end if
"#,
        launcher = applescript_launcher_command(script_file)
    )
}

/// macOS: Ghostty
#[cfg(target_os = "macos")]
fn launch_macos_ghostty(script_file: &std::path::Path) -> Result<(), String> {
    match run_terminal_osascript(&build_macos_ghostty_applescript(script_file), "Ghostty") {
        Ok(()) => Ok(()),
        Err(applescript_error) => {
            log::warn!(
                "Ghostty AppleScript launch failed, falling back to open -na: {applescript_error}"
            );
            launch_macos_open_app("Ghostty", script_file, true)
        }
    }
}

/// macOS: 使用 open -na 启动支持 --args 参数的终端（Alacritty/Kitty/WezTerm/Kaku）
#[cfg(target_os = "macos")]
fn launch_macos_open_app(
    app_name: &str,
    script_file: &std::path::Path,
    use_e_flag: bool,
) -> Result<(), String> {
    use std::process::Command;

    let mut cmd = Command::new("open");
    cmd.arg("-na").arg(app_name).arg("--args");

    if use_e_flag {
        cmd.arg("-e");
    }
    // Keep the script path inside `sh -c`; a trailing bare `.sh` can be opened as a document.
    cmd.arg("sh")
        .arg("-c")
        .arg(build_macos_dash_c_command(script_file));

    let output = cmd
        .output()
        .map_err(|e| format!("启动 {app_name} 失败: {e}"))?;

    if !output.status.success() {
        let stderr = decode_command_output(&output.stderr);
        return Err(format!(
            "{} 启动失败 (exit code: {:?}): {}",
            app_name,
            output.status.code(),
            stderr
        ));
    }

    Ok(())
}

#[cfg(target_os = "macos")]
fn launch_macos_warp(script_file: &std::path::Path) -> Result<(), String> {
    use std::io::Write;
    use std::os::unix::fs::PermissionsExt;
    use std::process::Command;

    let mut cmd = Command::new("open");
    cmd.arg("-a").arg("Warp");

    // Warp URI scheme cannot work well with script_file, because:
    //
    // 1. script_file's name ends up with .sh, so Warp would open the file rather than execute it
    // 2. script_file has no execution permission, so we need to add one more indirection
    let mut second_script_file = tempfile::Builder::new()
        .disable_cleanup(true)
        .permissions(std::fs::Permissions::from_mode(0o755))
        .tempfile()
        .map_err(|e| format!("Failed to create temporary script file: {e}"))?;

    writeln!(
        &mut second_script_file,
        r#"#!/usr/bin/env sh

        rm -- "$0"

        exec sh {quoted_script}
        "#,
        quoted_script = shell_single_quote(&script_file.to_string_lossy()),
    )
    .map_err(|e| format!("Failed to write to temporary script file for Warp: {e}"))?;

    let mut warp_url = url::Url::parse("warp://action/new_tab").unwrap();
    warp_url
        .query_pairs_mut()
        .append_pair("path", &second_script_file.path().to_string_lossy());
    let warp_url = warp_url.to_string();
    cmd.arg(warp_url);

    let output = cmd.output().map_err(|e| format!("启动 Warp 失败: {e}"))?;
    if !output.status.success() {
        let stderr = decode_command_output(&output.stderr);
        return Err(format!(
            "Warp 启动失败 (exit code: {:?}): {}",
            output.status.code(),
            stderr
        ));
    }

    Ok(())
}

/// Linux: 根据用户首选终端启动
#[cfg(target_os = "linux")]
fn launch_linux_terminal(script_file: &std::path::Path) -> Result<(), String> {
    use std::process::Command;

    let preferred = crate::settings::get_preferred_terminal();

    // Default terminal list with their arguments
    let default_terminals = [
        ("gnome-terminal", vec!["--"]),
        ("konsole", vec!["-e"]),
        ("xfce4-terminal", vec!["-e"]),
        ("mate-terminal", vec!["--"]),
        ("lxterminal", vec!["-e"]),
        ("alacritty", vec!["-e"]),
        ("kitty", vec!["-e"]),
        ("ghostty", vec!["-e"]),
    ];

    // Build terminal list: preferred terminal first (if specified), then defaults
    let terminals_to_try: Vec<(&str, Vec<&str>)> = if let Some(ref pref) = preferred {
        // Find the preferred terminal's args from default list
        let pref_args = default_terminals
            .iter()
            .find(|(name, _)| *name == pref.as_str())
            .map(|(_, args)| args.to_vec())
            .unwrap_or_else(|| vec!["-e"]); // Default args for unknown terminals

        let mut list = vec![(pref.as_str(), pref_args)];
        // Add remaining terminals as fallbacks
        for (name, args) in &default_terminals {
            if *name != pref.as_str() {
                list.push((*name, args.to_vec()));
            }
        }
        list
    } else {
        default_terminals
            .iter()
            .map(|(name, args)| (*name, args.to_vec()))
            .collect()
    };

    let mut last_error = String::from("未找到可用的终端");

    for (terminal, args) in terminals_to_try {
        // Check if terminal exists in common paths
        let terminal_exists = std::path::Path::new(&format!("/usr/bin/{}", terminal)).exists()
            || std::path::Path::new(&format!("/bin/{}", terminal)).exists()
            || std::path::Path::new(&format!("/usr/local/bin/{}", terminal)).exists()
            || which_command(terminal);

        if terminal_exists {
            let result = Command::new(terminal)
                .args(&args)
                .arg("sh")
                .arg(script_file.to_string_lossy().as_ref())
                .spawn();

            match result {
                Ok(_) => return Ok(()),
                Err(e) => {
                    last_error = format!("执行 {} 失败: {}", terminal, e);
                }
            }
        }
    }

    Err(last_error)
}

/// Check if a command exists using `which`
#[cfg(target_os = "linux")]
fn which_command(cmd: &str) -> bool {
    use std::process::Command;
    Command::new("which")
        .arg(cmd)
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Windows: 根据用户首选终端启动
#[cfg(target_os = "windows")]
fn launch_windows_terminal(launch: &ProviderLaunch, cwd: Option<&Path>) -> Result<(), String> {
    let preferred = crate::settings::get_preferred_terminal();
    let terminal = preferred.as_deref().unwrap_or("cmd");

    let bat_file = unique_temp_path("cc_switch_launcher", "bat");
    let (cwd_command, cwd_env) = build_windows_cwd_command(cwd);
    let mut launch_env: Vec<(String, String)> = cwd_env
        .map(|value| (WINDOWS_CWD_ENV.to_string(), value))
        .into_iter()
        .collect();
    let (set_lines, extra_env) = build_windows_launch_env(launch);
    launch_env.extend(extra_env);
    let launch_env: Vec<(&str, &str)> = launch_env
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect();
    let cleanup: String = launch
        .cleanup
        .iter()
        .filter_map(|file| file.file_name())
        .map(|name| {
            format!(
                "del \"%~dp0{}\" >nul 2>&1\r\n",
                escape_windows_batch_value(&name.to_string_lossy())
            )
        })
        .collect();

    // setlocal 段里设的变量（含 Key）在程序退出后随 endlocal 撤掉，不留在这个 cmd 会话里。
    // 注册表里开了延迟展开的机器上，值里的 `!名字!` 会被替换：这一段显式关掉。
    let content = format!(
        "@echo off\r\n{cwd_command}setlocal DisableDelayedExpansion\r\n{set_lines}echo {title}\r\n{command}\r\nendlocal\r\n{cleanup}del \"%~f0\" >nul 2>&1\r\n",
        title = escape_windows_batch_value(launch.title),
        command = launch.batch_command,
    );

    std::fs::write(&bat_file, &content).map_err(|e| format!("写入批处理文件失败: {e}"))?;

    let bat_path = bat_file.to_string_lossy();
    let ps_cmd = format!("& '{}'", bat_path);

    // Try the preferred terminal first
    let result = match terminal {
        "powershell" => run_windows_start_command_with_env(
            &[
                windows_powershell_program(&effective_path_string()),
                "-NoExit",
                "-Command",
                &ps_cmd,
            ],
            "PowerShell",
            &launch_env,
        ),
        "wt" => run_windows_start_command_with_env(
            &["wt", "cmd", "/K", &bat_path],
            "Windows Terminal",
            &launch_env,
        ),
        // "cmd" or default
        _ => run_windows_start_command_with_env(&["cmd", "/K", &bat_path], "cmd", &launch_env),
    };

    // If preferred terminal fails and it's not the default, try cmd as fallback
    if result.is_err() && terminal != "cmd" {
        log::warn!(
            "首选终端 {} 启动失败，回退到 cmd: {:?}",
            terminal,
            result.as_ref().err()
        );
        return run_windows_start_command_with_env(&["cmd", "/K", &bat_path], "cmd", &launch_env);
    }

    result
}

#[cfg_attr(windows, allow(dead_code))]
fn shell_single_quote(value: &str) -> String {
    format!("'{}'", value.replace('\'', "'\"'\"'"))
}

/// fish 的单引号里 `\'` 和 `\\` 仍是转义，POSIX 的 `'"'"'` 写法会让引号错位、接出别的命令。
#[cfg_attr(windows, allow(dead_code))]
fn is_fish_shell(shell: &str) -> bool {
    shell.rsplit('/').next().unwrap_or(shell) == "fish"
}

/// 交给用户 shell（而不是启动脚本的 sh）解析的一段文本加引号。
#[cfg_attr(windows, allow(dead_code))]
fn user_shell_quote(shell: &str, value: &str) -> String {
    if is_fish_shell(shell) {
        format!("'{}'", value.replace('\\', "\\\\").replace('\'', "\\'"))
    } else {
        shell_single_quote(value)
    }
}

/// `shell_word` 的用户 shell 版本：只含安全字符时原样写。
#[cfg_attr(windows, allow(dead_code))]
fn user_shell_word(shell: &str, value: &str) -> String {
    let word = shell_word(value);
    // zsh 把开头的 `=名字` 展开成命令路径（`=sh` → `/bin/sh`），找不到还会直接报错。
    if word == value && !value.starts_with('=') {
        word
    } else {
        user_shell_quote(shell, value)
    }
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn is_windows_unc_path(path: &str) -> bool {
    path.starts_with(r"\\")
}

/// .bat 里 `setlocal` 段的 `set` 行，以及要经启动进程环境传入的变量。纯 ASCII 的值直接
/// 写进文件（`%` 双写）；含非 ASCII 的按 #8034 的办法经环境变量转一手，免得按代码页乱码。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn build_windows_launch_env(launch: &ProviderLaunch) -> (String, Vec<(String, String)>) {
    let mut lines = String::new();
    let mut carried = Vec::new();
    let entries = launch
        .env
        .iter()
        .map(|(name, value)| (*name, value.clone()))
        .chain(
            launch
                .batch_vars
                .iter()
                .map(|(name, value)| (*name, Some(value.clone()))),
        );
    for (index, (name, value)) in entries.enumerate() {
        match value {
            None => lines.push_str(&format!("set \"{name}=\"\r\n")),
            Some(value) if value.is_ascii() => {
                lines.push_str(&format!("set \"{name}={}\"\r\n", value.replace('%', "%%")))
            }
            Some(value) => {
                let carrier = format!("CC_SWITCH_INTERNAL_ENV_{index}");
                lines.push_str(&format!(
                    "set \"{name}=%{carrier}%\"\r\nset \"{carrier}=\"\r\n"
                ));
                carried.push((carrier, value));
            }
        }
    }
    (lines, carried)
}

/// 非 ASCII 的项目目录经这个环境变量传给 Claude 启动 .bat。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
const WINDOWS_CWD_ENV: &str = "CC_SWITCH_INTERNAL_CWD";

/// 生成切到项目目录的 .bat 行，第二项是需要给启动进程设置的 `WINDOWS_CWD_ENV` 值。
///
/// cmd 按控制台代码页解码 .bat 字节，非 ASCII 路径以 UTF-8 写进文件会乱码（#8034）；
/// 环境变量在 Windows 内部保持 Unicode，所以这类路径改由环境变量传入。纯 ASCII
/// 路径照旧写进文件，不依赖终端继承环境变量（旧版 Windows Terminal 不继承）。
#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn build_windows_cwd_command_str(path: &str) -> (String, Option<String>) {
    let (target, env_value, cleanup) = if path.is_ascii() {
        (escape_windows_batch_value(path), None, String::new())
    } else {
        (
            format!("%{WINDOWS_CWD_ENV}%"),
            Some(path.to_string()),
            format!("set \"{WINDOWS_CWD_ENV}=\"\r\n"),
        )
    };

    let command = if is_windows_unc_path(path) {
        // `cmd.exe` cannot make a UNC path current via `cd`; `pushd` maps it first.
        format!("pushd \"{target}\" || exit /b 1\r\n{cleanup}")
    } else {
        format!("cd /d \"{target}\" || exit /b 1\r\n{cleanup}")
    };
    (command, env_value)
}

#[cfg(target_os = "windows")]
fn build_windows_cwd_command(cwd: Option<&Path>) -> (String, Option<String>) {
    cwd.map(|dir| build_windows_cwd_command_str(&dir.to_string_lossy()))
        .unwrap_or_default()
}

#[cfg_attr(not(target_os = "windows"), allow(dead_code))]
fn escape_windows_batch_value(value: &str) -> String {
    value
        .replace('^', "^^")
        .replace('%', "%%")
        .replace('&', "^&")
        .replace('|', "^|")
        .replace('<', "^<")
        .replace('>', "^>")
        .replace('(', "^(")
        .replace(')', "^)")
}
/// Windows: Run a start command with common error handling
#[cfg(target_os = "windows")]
fn run_windows_start_command(args: &[&str], terminal_name: &str) -> Result<(), String> {
    run_windows_start_command_with_env(args, terminal_name, &[])
}

/// 组装 `cmd /C start ...`。终端里跑的 `claude` 等命令按这里的 PATH 查找：CC Switch
/// 被 MSI 自动重启后只剩机器级 PATH（#6061），或是在装 CLI 之前就已启动，进程 PATH
/// 都会缺用户级条目（#3877、#8446）。和检测、生命周期命令（#8173）一样用补全后的 PATH，
/// 只影响启动的终端，不改 CC Switch 自身的环境；Windows Terminal 从命令行开新标签时
/// 也沿用调用方的环境。
#[cfg(target_os = "windows")]
fn windows_start_command(
    args: &[&str],
    envs: &[(&str, &str)],
    effective_path: &str,
) -> std::process::Command {
    let mut command = std::process::Command::new("cmd");
    command
        .arg("/C")
        .arg("start")
        .args(args)
        .env("PATH", effective_path)
        .envs(envs.iter().copied())
        .creation_flags(CREATE_NO_WINDOW);
    command
}

/// 同 `run_windows_start_command`，额外给启动的终端设置环境变量（`start` 会继承）。
///
/// stdio 指向 NUL、只等 `cmd` 的退出码，不能用 `output()` 捕获：`start` 打开的终端会
/// 继承这里的 stdout/stderr 管道句柄，管道要等那个窗口和里面的进程全部退出才到 EOF，
/// 调用就一直挂到用户关掉终端，界面也一直转圈（#6712）。启动失败仍能从退出码看出来。
#[cfg(target_os = "windows")]
fn run_windows_start_command_with_env(
    args: &[&str],
    terminal_name: &str,
    envs: &[(&str, &str)],
) -> Result<(), String> {
    use std::process::Stdio;

    let status = windows_start_command(args, envs, &effective_path_string())
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .map_err(|e| format!("启动 {} 失败: {e}", terminal_name))?;

    if !status.success() {
        return Err(format!(
            "{} 启动失败 (exit code: {:?})",
            terminal_name,
            status.code()
        ));
    }

    Ok(())
}

/// 首选终端为 PowerShell 时启动哪个程序。`powershell` 永远是系统自带的 Windows
/// PowerShell 5.1；装了 PowerShell 7+ 的用户默认用的是 `pwsh`，所以补全后的 PATH 里
/// 有 `pwsh.exe` 就用它，没有才退回 `powershell`。Microsoft Store 版的 `pwsh.exe` 是
/// App Execution Alias（跟随不了的重解析点），`Path::exists` 会判成不存在，这里用
/// `symlink_metadata`。
#[cfg(target_os = "windows")]
fn windows_powershell_program(path: &str) -> &'static str {
    let has_pwsh = std::env::split_paths(path)
        .filter(|dir| !dir.as_os_str().is_empty())
        .any(|dir| std::fs::symlink_metadata(dir.join("pwsh.exe")).is_ok());
    if has_pwsh {
        "pwsh"
    } else {
        "powershell"
    }
}

/// 打开用户首选终端并在其中执行一段可信命令脚本。脚本尾部 `read -r` / `pause`
/// 是刻意设计的——让命令退出后窗口不要瞬间关闭，用户才看得到 `command
/// not found` / `ModuleNotFoundError` 这类诊断信息。
///
/// **Security**：`command_line` 会被原样拼进 shell/batch 脚本，调用方必须
/// 保证它是可信字符串（当前只由后端硬编码调用）。
pub(crate) fn launch_terminal_running(command_line: &str, label: &str) -> Result<(), String> {
    let temp_dir = std::env::temp_dir();
    let pid = std::process::id();

    #[cfg(any(target_os = "macos", target_os = "linux"))]
    let (script_file, script_content) = {
        let file = temp_dir.join(format!("cc_switch_{}_{}.sh", label, pid));
        let content = format!(
            r#"#!/usr/bin/env sh
trap 'rm -f "{script_path}"' EXIT
echo "[cc-switch] Starting: {label}"
echo ""
{cmd}
echo ""
echo "[cc-switch] Command exited. Press Enter to close."
read -r _
"#,
            script_path = file.display(),
            label = label,
            cmd = command_line,
        );
        (file, content)
    };

    #[cfg(target_os = "macos")]
    {
        use std::os::unix::fs::PermissionsExt;

        std::fs::write(&script_file, &script_content)
            .map_err(|e| format!("写入启动脚本失败: {e}"))?;
        std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置脚本权限失败: {e}"))?;

        let preferred = crate::settings::get_preferred_terminal();
        let terminal = preferred.as_deref().unwrap_or("terminal");

        let result = match terminal {
            "iterm2" => launch_macos_iterm2(&script_file),
            "warp" => launch_macos_warp(&script_file),
            "alacritty" => launch_macos_open_app("Alacritty", &script_file, true),
            "kitty" => launch_macos_open_app("kitty", &script_file, false),
            "ghostty" => launch_macos_ghostty(&script_file),
            "otty" => launch_macos_otty(&script_file),
            "wezterm" => launch_macos_open_app("WezTerm", &script_file, true),
            "kaku" => launch_macos_open_app("Kaku", &script_file, true),
            _ => launch_macos_terminal_app(&script_file),
        };

        if result.is_err() && terminal != "terminal" {
            log::warn!(
                "首选终端 {} 启动失败，回退到 Terminal.app: {:?}",
                terminal,
                result.as_ref().err()
            );
            return launch_macos_terminal_app(&script_file);
        }
        result
    }

    #[cfg(target_os = "linux")]
    {
        use std::os::unix::fs::PermissionsExt;
        use std::process::Command;

        std::fs::write(&script_file, &script_content)
            .map_err(|e| format!("写入启动脚本失败: {e}"))?;
        std::fs::set_permissions(&script_file, std::fs::Permissions::from_mode(0o755))
            .map_err(|e| format!("设置脚本权限失败: {e}"))?;

        let preferred = crate::settings::get_preferred_terminal();
        let default_terminals = [
            ("gnome-terminal", vec!["--"]),
            ("konsole", vec!["-e"]),
            ("xfce4-terminal", vec!["-e"]),
            ("mate-terminal", vec!["--"]),
            ("lxterminal", vec!["-e"]),
            ("alacritty", vec!["-e"]),
            ("kitty", vec!["-e"]),
            ("ghostty", vec!["-e"]),
        ];

        let terminals_to_try: Vec<(&str, Vec<&str>)> = if let Some(ref pref) = preferred {
            let pref_args = default_terminals
                .iter()
                .find(|(name, _)| *name == pref.as_str())
                .map(|(_, args)| args.to_vec())
                .unwrap_or_else(|| vec!["-e"]);
            let mut list = vec![(pref.as_str(), pref_args)];
            for (name, args) in &default_terminals {
                if *name != pref.as_str() {
                    list.push((*name, args.to_vec()));
                }
            }
            list
        } else {
            default_terminals
                .iter()
                .map(|(name, args)| (*name, args.to_vec()))
                .collect()
        };

        let mut last_error = String::from("未找到可用的终端");

        for (terminal, args) in terminals_to_try {
            let terminal_exists = which_command(terminal)
                || ["/usr/bin", "/bin", "/usr/local/bin"]
                    .iter()
                    .any(|dir| std::path::Path::new(&format!("{}/{}", dir, terminal)).exists());

            if terminal_exists {
                let spawn_result = Command::new(terminal)
                    .args(&args)
                    .arg("sh")
                    .arg(script_file.to_string_lossy().as_ref())
                    .spawn();
                match spawn_result {
                    Ok(_) => return Ok(()),
                    Err(e) => {
                        last_error = format!("执行 {} 失败: {}", terminal, e);
                    }
                }
            }
        }

        let _ = std::fs::remove_file(&script_file);
        Err(last_error)
    }

    #[cfg(target_os = "windows")]
    {
        let preferred = crate::settings::get_preferred_terminal();
        let terminal = preferred.as_deref().unwrap_or("cmd");

        let bat_file = temp_dir.join(format!("cc_switch_{}_{}.bat", label, pid));
        let content = format!(
            "@echo off\r\necho [cc-switch] Starting: {label}\r\necho.\r\n{cmd}\r\necho.\r\necho [cc-switch] Command exited. Press any key to close.\r\npause >nul\r\ndel \"%~f0\" >nul 2>&1\r\n",
            label = label,
            cmd = command_line,
        );
        std::fs::write(&bat_file, &content).map_err(|e| format!("写入批处理文件失败: {e}"))?;

        let bat_path = bat_file.to_string_lossy();
        let ps_cmd = format!("& '{}'", bat_path);

        let result = match terminal {
            "powershell" => run_windows_start_command(
                &[
                    windows_powershell_program(&effective_path_string()),
                    "-NoExit",
                    "-Command",
                    &ps_cmd,
                ],
                "PowerShell",
            ),
            "wt" => run_windows_start_command(&["wt", "cmd", "/K", &bat_path], "Windows Terminal"),
            _ => run_windows_start_command(&["cmd", "/K", &bat_path], "cmd"),
        };

        let final_result = if result.is_err() && terminal != "cmd" {
            log::warn!(
                "首选终端 {} 启动失败，回退到 cmd: {:?}",
                terminal,
                result.as_ref().err()
            );
            run_windows_start_command(&["cmd", "/K", &bat_path], "cmd")
        } else {
            result
        };

        // The .bat self-deletes (`del "%~f0"`) after it runs, but that only
        // fires if *some* terminal actually launched it. If every attempt
        // failed, sweep the temp file ourselves to avoid pollution.
        if final_result.is_err() {
            let _ = std::fs::remove_file(&bat_file);
        }
        final_result
    }

    #[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
    {
        let _ = (temp_dir, pid, command_line, label);
        Err("不支持的操作系统".to_string())
    }
}

/// 设置窗口主题（Windows/macOS 标题栏颜色）
/// theme: "dark" | "light" | "system"
#[tauri::command]
pub async fn set_window_theme(window: tauri::Window, theme: String) -> Result<(), String> {
    use tauri::Theme;

    let tauri_theme = match theme.as_str() {
        "dark" => Some(Theme::Dark),
        "light" => Some(Theme::Light),
        _ => None, // system default
    };

    window.set_theme(tauri_theme).map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::tool_discovery::*;
    use super::tool_lifecycle_command::*;
    use super::tool_process::*;
    use super::tool_version::*;

    #[test]
    fn shell_command_missing_matches_shell_formats_only() {
        assert!(is_shell_command_missing(
            "bash: line 1: claude: command not found",
            "claude"
        ));
        assert!(is_shell_command_missing(
            "zsh:1: command not found: claude",
            "claude"
        ));
        assert!(is_shell_command_missing(
            "sh: 1: claude: not found",
            "claude"
        ));
        assert!(is_shell_command_missing(
            "fish: Unknown command: claude",
            "claude"
        ));
        assert!(!is_shell_command_missing(
            "Error: Cannot find module 'x'\nmodule not found",
            "claude"
        ));
        assert!(!is_shell_command_missing("config file not found", "claude"));
    }
    use super::*;

    #[test]
    fn portable_zip_url_matches_release_asset_names() {
        assert_eq!(
            portable_zip_url("4.0.12", false),
            Some(format!(
                "{REPO_URL}/releases/download/v4.0.12/CC-Switch-v4.0.12-Windows-Portable.zip"
            ))
        );
        assert_eq!(
            portable_zip_url("v4.1.0-beta.1", true),
            Some(format!(
                "{REPO_URL}/releases/download/v4.1.0-beta.1/CC-Switch-v4.1.0-beta.1-Windows-arm64-Portable.zip"
            ))
        );
        for bad in ["", "v", "latest", "4.0/../x", "4.0.0?x=1"] {
            assert!(portable_zip_url(bad, false).is_none(), "{bad}");
        }
    }

    #[test]
    fn external_url_whitelist() {
        assert_eq!(
            validate_external_url("https://example.com/a").unwrap(),
            "https://example.com/a"
        );
        assert!(validate_external_url("http://localhost:3000").is_ok());
        assert!(validate_external_url("mailto:a@example.com").is_ok());
        assert_eq!(
            validate_external_url("example.com/docs").unwrap(),
            "https://example.com/docs"
        );
        for bad in [
            "javascript:alert(1)",
            "JavaScript:alert(1)",
            " javascript:alert(1)",
            "file:///etc/passwd",
            "data:text/html,<script>alert(1)</script>",
            "vscode://open",
            "https://",
            "",
        ] {
            assert!(validate_external_url(bad).is_err(), "{bad}");
        }
    }
    use std::path::{Path, PathBuf};

    #[tokio::test]
    async fn lifecycle_coordinator_serializes_writes_and_rejects_duplicate_tools() {
        let coordinator = Arc::new(ToolLifecycleCoordinator::default());
        let output_dir = tempfile::tempdir().unwrap();
        let output = output_dir.path().join("installed.txt");
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let first = {
            let coordinator = coordinator.clone();
            let output = output.clone();
            tokio::spawn(async move {
                coordinator
                    .run(vec!["codex"], move |_| {
                        started_tx.send(()).unwrap();
                        finish_rx.recv().unwrap();
                        std::fs::write(output, "codex").unwrap();
                        Ok(())
                    })
                    .await
            })
        };
        started_rx.await.unwrap();

        // 独立调用者（例如重挂后的页面）不能重复启动正在执行的工具。
        assert_eq!(
            coordinator
                .run(vec!["codex"], |_| panic!("duplicate must not execute"))
                .await
                .unwrap_err(),
            "TOOL_ACTION_IN_PROGRESS"
        );
        // 批次部分取锁失败时，已取得的其他工具锁也必须释放。
        assert!(coordinator
            .run(vec!["claude", "codex"], |_| panic!(
                "batch must not execute"
            ))
            .await
            .is_err());

        let second_output = output.clone();
        let second = coordinator.run(vec!["claude"], move |_| {
            assert_eq!(std::fs::read_to_string(&second_output).unwrap(), "codex");
            std::fs::write(second_output, "codex,claude").unwrap();
            Ok(())
        });
        tokio::pin!(second);
        assert!(futures::poll!(second.as_mut()).is_pending());
        assert!(!output.exists(), "first write has not finished yet");
        assert_eq!(
            coordinator
                .run(vec!["claude"], |_| panic!(
                    "queued duplicate must not execute"
                ))
                .await
                .unwrap_err(),
            "TOOL_ACTION_IN_PROGRESS"
        );

        finish_tx.send(()).unwrap();
        first.await.unwrap().unwrap();
        second.await.unwrap();
        assert_eq!(std::fs::read_to_string(output).unwrap(), "codex,claude");
    }

    #[tokio::test]
    async fn lifecycle_coordinator_keeps_running_locks_when_caller_is_cancelled() {
        let coordinator = Arc::new(ToolLifecycleCoordinator::default());
        let (started_tx, started_rx) = tokio::sync::oneshot::channel();
        let (finish_tx, finish_rx) = std::sync::mpsc::channel();
        let first = {
            let coordinator = coordinator.clone();
            tokio::spawn(async move {
                coordinator
                    .run(vec!["codex"], move |_| {
                        started_tx.send(()).unwrap();
                        finish_rx.recv().unwrap();
                        Ok(())
                    })
                    .await
            })
        };
        started_rx.await.unwrap();
        first.abort();
        assert!(first.await.unwrap_err().is_cancelled());
        assert!(coordinator
            .run(vec!["codex"], |_| panic!(
                "running duplicate must not execute"
            ))
            .await
            .is_err());

        {
            let cancelled = coordinator.run(vec!["claude"], |_| {
                panic!("cancelled queued operation must not execute")
            });
            tokio::pin!(cancelled);
            assert!(futures::poll!(cancelled.as_mut()).is_pending());
        }
        // 尚未开始的排队请求取消后可以重试，但仍须等待正在运行的子任务退出。
        let retry = coordinator.run(vec!["claude"], |_| Ok(()));
        tokio::pin!(retry);
        assert!(futures::poll!(retry.as_mut()).is_pending());
        finish_tx.send(()).unwrap();
        retry.await.unwrap();
        coordinator.run(vec!["codex"], |_| Ok(())).await.unwrap();
    }

    #[tokio::test]
    async fn lifecycle_coordinator_releases_locks_after_error_or_panic() {
        let coordinator = ToolLifecycleCoordinator::default();
        assert_eq!(
            coordinator
                .run(vec!["claude"], |_| Err("installer failed".to_string()))
                .await,
            Err("installer failed".to_string())
        );
        coordinator.run(vec!["claude"], |_| Ok(())).await.unwrap();
        assert!(coordinator
            .run(vec!["claude"], |_| panic!("installer panicked"))
            .await
            .unwrap_err()
            .contains("tool lifecycle task join error"));
        coordinator.run(vec!["claude"], |_| Ok(())).await.unwrap();
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn concurrent_lifecycle_runs_use_independent_scripts() {
        let output_dir = tempfile::tempdir().unwrap();
        let outputs = [
            output_dir.path().join("first.txt"),
            output_dir.path().join("second.txt"),
        ];
        let barrier = std::sync::Barrier::new(2);
        std::thread::scope(|scope| {
            for output in &outputs {
                let barrier = &barrier;
                scope.spawn(move || {
                    let command = format!("@echo off\r\n>\"{}\" echo %~f0\r\n", output.display());
                    barrier.wait();
                    run_tool_lifecycle_silently(&command, "tool_update").unwrap();
                });
            }
        });
        let script_paths =
            outputs.map(|output| PathBuf::from(std::fs::read_to_string(output).unwrap().trim()));
        assert_ne!(script_paths[0], script_paths[1]);
        for script_path in script_paths {
            assert!(!script_path.exists(), "temporary script must be cleaned up");
            assert!(
                !script_path.parent().unwrap().exists(),
                "temporary directory must be cleaned up"
            );
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_powershell_program_prefers_pwsh_on_path() {
        let root = tempfile::tempdir().unwrap();
        let empty = root.path().join("empty");
        let pwsh_dir = root.path().join("PowerShell 7");
        std::fs::create_dir_all(&empty).unwrap();
        std::fs::create_dir_all(&pwsh_dir).unwrap();
        std::fs::write(pwsh_dir.join("pwsh.exe"), "").unwrap();

        let without = empty.to_str().unwrap();
        assert_eq!(windows_powershell_program(without), "powershell");
        assert_eq!(windows_powershell_program(""), "powershell");

        let with = format!("{};{}", empty.display(), pwsh_dir.display());
        assert_eq!(windows_powershell_program(&with), "pwsh");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_lifecycle_restores_user_path_for_npm_and_its_child() {
        let root = tempfile::tempdir().unwrap();
        let npm_dir = root.path().join("npm 用户 & tools");
        let node_dir = root.path().join("node 用户 tools");
        std::fs::create_dir_all(&npm_dir).unwrap();
        std::fs::create_dir_all(&node_dir).unwrap();
        std::fs::write(
            npm_dir.join("npm.cmd"),
            "@echo off\r\ncmd /D /C cc_switch_lifecycle_test_node.cmd\r\nexit /b %errorlevel%\r\n",
        )
        .unwrap();
        std::fs::write(
            node_dir.join("cc_switch_lifecycle_test_node.cmd"),
            "@echo restored-user-path\r\n@exit /b 0\r\n",
        )
        .unwrap();
        let script = root.path().join("update.bat");
        std::fs::write(&script, "@echo off\r\ncall npm --version\r\n").unwrap();

        // Model an MSI-launched process with only the system directory on PATH.
        // Inject the registry contribution without modifying the real registry
        // or the test runner's process environment.
        let system_path = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
        let inherited = system_path.to_str().unwrap();
        let user_path = format!("{};{}", npm_dir.display(), node_dir.display());
        let effective = merge_path_segments_win(&[inherited, &user_path]);
        let parent_path = std::env::var_os("PATH");

        let missing = windows_lifecycle_command(&script, inherited)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            !missing.status.success(),
            "npm must be absent on the narrow PATH"
        );

        let restored = windows_lifecycle_command(&script, &effective)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(restored.status.success(), "{restored:?}");
        assert_eq!(
            decode_command_output(&restored.stdout).trim(),
            "restored-user-path"
        );
        assert_eq!(std::env::var_os("PATH"), parent_path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_start_command_passes_restored_path_to_the_terminal() {
        let root = tempfile::tempdir().unwrap();
        let tool_dir = root.path().join("claude 用户 & bin");
        std::fs::create_dir_all(&tool_dir).unwrap();
        std::fs::write(
            tool_dir.join("cc_switch_start_test_tool.cmd"),
            "@echo restored-terminal-path\r\n@exit /b 0\r\n",
        )
        .unwrap();

        // `start /B /WAIT` runs the child in the same hidden console, so the
        // test observes what the launched terminal would resolve.
        let args = [
            "/B",
            "/WAIT",
            "cmd",
            "/D",
            "/C",
            "cc_switch_start_test_tool",
        ];
        let system_path = PathBuf::from(std::env::var_os("SystemRoot").unwrap()).join("System32");
        let inherited = system_path.to_str().unwrap();
        let effective = merge_path_segments_win(&[inherited, tool_dir.to_str().unwrap()]);
        let parent_path = std::env::var_os("PATH");

        let missing = windows_start_command(&args, &[], inherited)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            !decode_command_output(&missing.stdout).contains("restored-terminal-path"),
            "the tool must be absent on the narrow PATH: {missing:?}"
        );

        let restored = windows_start_command(&args, &[], &effective)
            .current_dir(root.path())
            .output()
            .unwrap();
        assert!(
            decode_command_output(&restored.stdout).contains("restored-terminal-path"),
            "{restored:?}"
        );
        assert_eq!(std::env::var_os("PATH"), parent_path);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_lifecycle_preserves_process_path_priority_and_failure() {
        let root = tempfile::tempdir().unwrap();
        let preferred = root.path().join("process tools");
        let fallback = root.path().join("registry tools");
        std::fs::create_dir_all(&preferred).unwrap();
        std::fs::create_dir_all(&fallback).unwrap();
        std::fs::write(
            preferred.join("npm.cmd"),
            "@echo process-npm-failed 1>&2\r\n@exit /b 23\r\n",
        )
        .unwrap();
        std::fs::write(
            fallback.join("npm.cmd"),
            "@echo registry-npm\r\n@exit /b 0\r\n",
        )
        .unwrap();
        let script = root.path().join("update.bat");
        std::fs::write(&script, "@echo off\r\ncall npm --version\r\n").unwrap();
        let effective =
            merge_path_segments_win(&[preferred.to_str().unwrap(), fallback.to_str().unwrap()]);

        // 断言整段 stderr：临时目录在 \\wsl.localhost 下时（WSL2 nightly）以它为工作目录，
        // cmd 会先往 stderr 打一段「UNC paths are not supported」，所以换成系统目录
        let output = windows_lifecycle_command(&script, &effective)
            .current_dir(std::env::var_os("SystemRoot").unwrap())
            .output()
            .unwrap();
        assert_eq!(output.status.code(), Some(23));
        assert_eq!(
            finish_lifecycle_output(&output).unwrap_err(),
            "process-npm-failed"
        );
    }

    /// npm bin 路径冲突（#9173）：EEXIST + File exists 行齐备时，错误详情后
    /// 追加指出冲突路径与安全处理方式的提示，且不建议盲目 --force。
    #[test]
    fn npm_bin_collision_appends_actionable_hint() {
        let raw = "npm error code EEXIST\n\
                   npm error path /home/user/.local/bin/pi\n\
                   npm error EEXIST: file already exists\n\
                   npm error File exists: /home/user/.local/bin/pi\n\
                   npm error Remove the existing file and try again, or run npm\n\
                   npm error with --force to overwrite files recklessly.";
        let detail = append_npm_bin_collision_hint("npm error code EEXIST".to_string(), raw);
        assert!(detail.contains("/home/user/.local/bin/pi"));
        assert!(detail.contains("/home/user/.local/bin/pi.bak"));
        assert!(detail.contains("Do not use npm --force"));
    }

    /// Windows 的 npm 输出同样带 `File exists:` 行（含盘符路径），应能提取。
    #[test]
    fn npm_bin_collision_extracts_windows_path() {
        let raw = "npm error code EEXIST\nnpm error File exists: C:\\Users\\me\\AppData\\Roaming\\npm\\pi.cmd";
        assert_eq!(
            npm_bin_collision_path(raw),
            Some("C:\\Users\\me\\AppData\\Roaming\\npm\\pi.cmd")
        );
    }

    /// 只有 EEXIST 字样、没有 File exists 行的失败不误判，原样返回。
    #[test]
    fn npm_bin_collision_ignores_other_failures() {
        let raw = "some tool failed with EEXIST while writing its cache";
        let detail = "cache write failed".to_string();
        assert_eq!(append_npm_bin_collision_hint(detail.clone(), raw), detail);
        assert_eq!(npm_bin_collision_path("npm error code E404"), None);
    }

    #[test]
    fn npm_bin_collision_detail_survives_trailing_noise() {
        let raw = [
            "npm error code EEXIST",
            "npm error path /home/me/.local/bin/pi",
            "npm error EEXIST: file already exists",
            "npm error File exists: /home/me/.local/bin/pi",
            "npm error Remove the existing file and try again, or run npm",
            "npm error with --force to overwrite files recklessly.",
            "npm notice",
            "npm notice New major version of npm available! 10.9.2 -> 11.6.0",
            "npm notice Changelog: https://github.com/npm/cli/releases/tag/v11.6.0",
            "npm notice To update run: npm install -g npm@11.6.0",
            "npm notice",
            "npm error A complete log of this run can be found in: /tmp/debug-0.log",
        ]
        .join("\n");
        assert_eq!(
            npm_bin_collision_detail(&raw).as_deref(),
            Some("npm error code EEXIST\nnpm error File exists: /home/me/.local/bin/pi")
        );
        assert_eq!(npm_bin_collision_detail("npm error code E404"), None);
        assert_eq!(npm_bin_collision_detail("EEXIST without a path"), None);
    }

    /// 探测 helper 正常路径：spawn（含 pre_exec setsid）能启动、输出能捕获。
    /// `/bin/echo --version` 在 macOS/Linux 均即刻成功退出。
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn probe_version_command_captures_healthy_tool_output() {
        let out = run_probe_version_command(Path::new("/bin/echo"), std::ffi::OsStr::new(""))
            .expect("probe of /bin/echo should succeed");
        assert!(out.status.success());
    }

    /// 超时击杀路径：挂死的子进程到点被整组击杀、wait 返回超时错误而非永等。
    /// 同时锚定 setsid 改造后的语义——child 是新会话/新进程组组长，
    /// terminate_child_tree 的 kill(-pid) 仍能命中（回归红线：改回 process_group
    /// 或去掉隔离都会让本测试的击杀路径失效）。
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn isolated_hung_child_is_killed_on_deadline() {
        use std::process::{Command, Stdio};

        let mut cmd = Command::new("/bin/sh");
        cmd.args(["-c", "sleep 30"])
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        isolate_child_process_group(&mut cmd);
        let child = cmd.spawn().expect("spawn sleep");
        let started = std::time::Instant::now();
        let result = wait_child_output(
            child,
            CommandDeadline::from_timeout(Some(std::time::Duration::from_millis(200))),
        );
        assert!(result.is_err(), "expected timeout error, got {result:?}");
        assert!(
            started.elapsed() < std::time::Duration::from_secs(5),
            "kill should return promptly instead of waiting out the sleep"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn wsl_env_allows_spaces_in_unc_config_path() {
        let extra_env = [
            (
                "OPENCODE_CONFIG_DIR",
                r"\\wsl$\Ubuntu\home\Jane Doe\.config\opencode".to_string(),
            ),
            ("OPENCODE_DISABLE_PROJECT_CONFIG", "true".to_string()),
        ];

        assert_eq!(
            build_wsl_env_argv(&extra_env).unwrap(),
            vec![
                "OPENCODE_CONFIG_DIR=/home/Jane Doe/.config/opencode".to_string(),
                "OPENCODE_DISABLE_PROJECT_CONFIG=true".to_string(),
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn wsl_env_skips_host_config_path() {
        let extra_env = [
            (
                "OPENCODE_CONFIG_DIR",
                r"C:\Users\Jane Doe\.config\opencode".to_string(),
            ),
            ("OPENCODE_DISABLE_PROJECT_CONFIG", "true".to_string()),
        ];

        assert_eq!(
            build_wsl_env_argv(&extra_env).unwrap(),
            vec!["OPENCODE_DISABLE_PROJECT_CONFIG=true".to_string()]
        );
    }

    #[cfg(unix)]
    fn set_test_executable(path: &Path, executable: bool) {
        use std::os::unix::fs::PermissionsExt;

        let mode = if executable { 0o755 } else { 0o644 };
        std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode))
            .expect("fixture permissions should be set");
    }

    #[test]
    fn test_build_exec_line() {
        assert_eq!(build_exec_line("/bin/zsh", None), "exec '/bin/zsh' -l");
        assert_eq!(build_exec_line("/bin/bash", None), "exec '/bin/bash'");
        assert_eq!(
            build_exec_line("/opt/homebrew dir/bin/fish", None),
            "exec '/opt/homebrew dir/bin/fish'"
        );
        assert_eq!(build_exec_line("/bin/sh", None), "exec '/bin/sh'");
        assert_eq!(
            build_exec_line("/tmp/shell'quote/zsh", None),
            "exec '/tmp/shell'\"'\"'quote/zsh' -l"
        );
        assert_eq!(
            build_exec_line("/bin/zsh", Some(Path::new("/tmp/project"))),
            r#"exec '/bin/zsh' -lc 'cd '"'"'/tmp/project'"'"' || exit 1; exec '"'"'/bin/zsh'"'"' -i'"#
        );
    }

    #[test]
    fn test_build_provider_command_line_uses_user_shell_environment() {
        let claude = format!(
            "claude --settings {}",
            shell_word("/tmp/claude config.json")
        );
        assert_eq!(
            build_provider_command_line("/bin/zsh", &claude, None),
            "'/bin/zsh' -lic 'claude --settings '\"'\"'/tmp/claude config.json'\"'\"''"
        );
        assert_eq!(
            build_provider_command_line("/bin/bash", &claude, Some(Path::new("/tmp/project"))),
            r#"'/bin/bash' -ic 'cd '"'"'/tmp/project'"'"' && claude --settings '"'"'/tmp/claude config.json'"'"''"#
        );
        assert_eq!(
            build_provider_command_line(
                "/bin/sh",
                &claude,
                Some(Path::new("/tmp/project O'Brien"))
            ),
            r#"'/bin/sh' -c 'cd '"'"'/tmp/project O'"'"'"'"'"'"'"'"'Brien'"'"' && claude --settings '"'"'/tmp/claude config.json'"'"''"#
        );
    }

    fn agy_key_launch() -> ProviderLaunch {
        ProviderLaunch {
            title: "Using a private Antigravity profile for this provider:",
            detail: Some(PathBuf::from("/home/me/.cc-switch/agy/relay")),
            program: "agy",
            args: vec!["--gemini_dir=/home/me/.cc-switch/agy/relay".to_string()],
            batch_command: format!("agy \"--gemini_dir=%{AGY_DIR_BATCH_VAR}%\""),
            env: vec![
                ("GEMINI_API_KEY", Some("k'$(echo no)`x`%PATH%".to_string())),
                ("GOOGLE_GEMINI_BASE_URL", None),
            ],
            batch_vars: vec![(
                AGY_DIR_BATCH_VAR,
                r"C:\Users\张三\.cc-switch\agy\relay".to_string(),
            )],
            cleanup: Vec::new(),
        }
    }

    #[test]
    fn launch_command_keeps_secrets_out_of_the_command_line() {
        let launch = agy_key_launch();
        assert_eq!(
            unix_launch_command(&launch),
            "env -u CC_SWITCH_LAUNCH_ENV_0 -u CC_SWITCH_LAUNCH_ENV_1 -u GOOGLE_GEMINI_BASE_URL GEMINI_API_KEY=\"$CC_SWITCH_LAUNCH_ENV_0\" agy --gemini_dir=/home/me/.cc-switch/agy/relay"
        );
        assert_eq!(
            unix_launch_env_file(&launch),
            "CC_SWITCH_LAUNCH_ENV_0='k'\"'\"'$(echo no)`x`%PATH%'\n"
        );
        let plain = ProviderLaunch {
            env: Vec::new(),
            ..agy_key_launch()
        };
        assert_eq!(
            unix_launch_command(&plain),
            "agy --gemini_dir=/home/me/.cc-switch/agy/relay"
        );
        assert_eq!(unix_launch_env_file(&plain), "");
    }

    #[test]
    fn split_launch_args_groups_quoted_values_without_a_shell() {
        assert_eq!(split_launch_args("  ").unwrap(), Vec::<String>::new());
        assert_eq!(
            split_launch_args(
                "--debug --append-system-prompt \"be brief; ok\" --mcp-config 'C:\\a b\\mcp.json'\n-x=\"\""
            )
            .unwrap(),
            vec![
                "--debug",
                "--append-system-prompt",
                "be brief; ok",
                "--mcp-config",
                "C:\\a b\\mcp.json",
                "-x=",
            ]
        );
        // 引号可以接在词中间，引号里也能放另一种引号
        assert_eq!(
            split_launch_args("--name=\"it's\" ''").unwrap(),
            vec!["--name=it's", ""]
        );
        assert!(split_launch_args("--append-system-prompt \"oops").is_err());
        assert!(split_launch_args("\"line\nbreak\"").is_err());
        let too_many = vec!["-v"; USER_ARG_BATCH_VARS.len() + 1].join(" ");
        assert!(split_launch_args(&too_many).is_err());
    }

    #[test]
    fn fish_gets_its_own_quoting_for_user_text() {
        let mut launch = claude_account_terminal_launch(None);
        launch.env.clear();
        launch
            .append_user_args(vec!["x\\'; printf INJECTED; #".into(), "C:\\work\\".into()])
            .unwrap();
        assert_eq!(
            unix_launch_command_in(&launch, "/opt/homebrew/bin/fish"),
            "claude 'x\\\\\\'; printf INJECTED; #' 'C:\\\\work\\\\'"
        );
        assert_eq!(
            unix_launch_command_in(&launch, "/bin/zsh"),
            "claude 'x\\'\"'\"'; printf INJECTED; #' 'C:\\work\\'"
        );
        // .bat 里收尾的反斜杠成对，Unix 参数保持原样
        assert_eq!(launch.batch_vars[1].1, "C:\\work\\\\");
        assert_eq!(launch.args[1], "C:\\work\\");
        assert_eq!(
            build_provider_command_line("/usr/bin/fish", "claude", Some(Path::new("/tmp/a\\'b"))),
            "'/usr/bin/fish' -ic 'cd '\"'\"'/tmp/a\\\\\\'\"'\"'b'\"'\"' && claude'"
        );
        let mut launch = claude_account_terminal_launch(None);
        launch.env.clear();
        launch.append_user_args(vec!["=sh".into()]).unwrap();
        assert_eq!(unix_launch_command_in(&launch, "/bin/zsh"), "claude '=sh'");
    }

    #[test]
    fn user_args_follow_the_generated_args_and_pass_through_batch_vars() {
        let mut launch = claude_account_terminal_launch(None);
        launch.env.clear();
        launch.skip_permissions();
        launch
            .append_user_args(vec!["--append-system-prompt".into(), "a & b | c".into()])
            .unwrap();
        assert_eq!(
            unix_launch_command(&launch),
            "claude --dangerously-skip-permissions --append-system-prompt 'a & b | c'"
        );
        assert_eq!(
            launch.batch_command,
            "claude --dangerously-skip-permissions \"%CC_SWITCH_ARG0%\" \"%CC_SWITCH_ARG1%\""
        );
        assert_eq!(
            launch.batch_vars,
            vec![
                ("CC_SWITCH_ARG0", "--append-system-prompt".to_string()),
                ("CC_SWITCH_ARG1", "a & b | c".to_string()),
            ]
        );
    }

    #[test]
    fn skip_permissions_appends_the_flag_on_every_platform() {
        let mut launch = claude_account_terminal_launch(None);
        launch.env.clear();
        launch.skip_permissions();
        assert_eq!(
            unix_launch_command(&launch),
            "claude --dangerously-skip-permissions"
        );
        assert_eq!(
            launch.batch_command,
            "claude --dangerously-skip-permissions"
        );

        let mut launch = agy_key_launch();
        launch.skip_permissions();
        assert!(unix_launch_command(&launch).ends_with(
            "agy --gemini_dir=/home/me/.cc-switch/agy/relay --dangerously-skip-permissions"
        ));
        assert_eq!(
            launch.batch_command,
            format!("agy \"--gemini_dir=%{AGY_DIR_BATCH_VAR}%\" --dangerously-skip-permissions")
        );
    }

    #[test]
    fn codex_launch_passes_overrides_as_separate_arguments() {
        let mut launch = codex_launch(
            "Kimi",
            vec![
                "model_provider='custom'".to_string(),
                "model_providers.custom={name='K',base_url='https://k.example/v1?a=1&b=2'}"
                    .to_string(),
            ],
            vec![("CC_SWITCH_CODEX_API_KEY", Some("sk-kimi".to_string()))],
            Vec::new(),
        )
        .unwrap();
        launch.skip_permissions();
        assert_eq!(
            launch.args,
            vec![
                "-c",
                "model_provider='custom'",
                "-c",
                "model_providers.custom={name='K',base_url='https://k.example/v1?a=1&b=2'}",
                "--dangerously-bypass-approvals-and-sandbox",
            ]
        );
        assert!(!unix_launch_command(&launch).contains("sk-kimi"));
        assert_eq!(
            launch.batch_command,
            "call codex -c \"%CC_SWITCH_CODEX_C0%\" -c \"%CC_SWITCH_CODEX_C1%\" --dangerously-bypass-approvals-and-sandbox"
        );
        assert_eq!(launch.batch_vars[1].0, "CC_SWITCH_CODEX_C1");

        let plain = codex_launch("Kimi", Vec::new(), Vec::new(), Vec::new()).unwrap();
        assert_eq!(plain.batch_command, "call codex");
        assert!(plain.args.is_empty());
    }

    #[test]
    fn agy_login_launch_picks_the_model_for_this_session_only() {
        let mut row = crate::provider::Provider::with_id(
            "login".into(),
            "Google".into(),
            serde_json::json!({"env":{},"config":{"model":{"name":"Gemini 3.1 Pro (High)"}}}),
            None,
        );
        row.category = Some("official".into());
        let launch = agy_terminal_launch(&row).unwrap();
        assert!(launch.env.is_empty());
        assert_eq!(launch.detail, None);
        assert_eq!(
            unix_launch_command(&launch),
            "agy '--model=Gemini 3.1 Pro (High)'"
        );
        assert_eq!(
            launch.batch_command,
            format!("agy \"--model=%{AGY_MODEL_BATCH_VAR}%\"")
        );
        assert_eq!(
            launch.batch_vars,
            vec![(AGY_MODEL_BATCH_VAR, "Gemini 3.1 Pro (High)".to_string())]
        );
        // 带了 `--model` 的官方卡也没有私有目录，会话恢复不会把它当成 Key 卡（在开终端前就报错）。
        assert!(open_agy_resume_terminal(&row, "abc", None).is_err());

        row.settings_config = serde_json::json!({"env":{},"config":{}});
        let launch = agy_terminal_launch(&row).unwrap();
        assert_eq!(unix_launch_command(&launch), "agy");
        assert_eq!(launch.batch_command, "agy");
    }

    #[cfg(unix)]
    #[test]
    fn unix_launcher_script_passes_env_past_rc_files_and_cleans_up() {
        use std::os::unix::fs::PermissionsExt;
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("home");
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&home).unwrap();
        std::fs::create_dir_all(&bin).unwrap();
        // A user rc that exports its own Gemini variables must not win.
        std::fs::write(
            home.join(".bashrc"),
            "export GEMINI_API_KEY=from-rc\nexport GOOGLE_GEMINI_BASE_URL=https://rc.test\n",
        )
        .unwrap();
        let out = tmp.path().join("out");
        std::fs::write(
            bin.join("agy"),
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"${{GEMINI_API_KEY-unset}}\" \"${{GOOGLE_GEMINI_BASE_URL-unset}}\" \"${{CC_SWITCH_LAUNCH_ENV_0-unset}}\" \"$@\" > '{}'\n",
                out.display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(bin.join("agy"), std::fs::Permissions::from_mode(0o755)).unwrap();

        let launch = agy_key_launch();
        let script = tmp.path().join("launcher.sh");
        let env_file = tmp.path().join("env.sh");
        std::fs::write(&env_file, unix_launch_env_file(&launch)).unwrap();
        // Replace the final interactive shell with a no-op so the test returns.
        let content =
            build_unix_launcher_script(&launch, "/bin/bash", None, &script, Some(&env_file))
                .replace("exec '/bin/bash'", "exit 0 #");
        std::fs::write(&script, content).unwrap();
        let status = std::process::Command::new("sh")
            .arg(&script)
            .env("HOME", &home)
            .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
            .env("GEMINI_API_KEY", "from-gui")
            .status()
            .unwrap();
        assert!(status.success());
        assert_eq!(
            std::fs::read_to_string(&out).unwrap(),
            "k'$(echo no)`x`%PATH%\nunset\nunset\n--gemini_dir=/home/me/.cc-switch/agy/relay\n"
        );
        assert!(
            !env_file.exists(),
            "env file is removed right after reading"
        );
        assert!(!script.exists());
    }

    #[test]
    fn windows_launch_env_scopes_values_and_carries_non_ascii_paths() {
        let (lines, carried) = build_windows_launch_env(&agy_key_launch());
        assert_eq!(
            lines,
            "set \"GEMINI_API_KEY=k'$(echo no)`x`%%PATH%%\"\r\n\
             set \"GOOGLE_GEMINI_BASE_URL=\"\r\n\
             set \"CC_SWITCH_AGY_DIR=%CC_SWITCH_INTERNAL_ENV_2%\"\r\n\
             set \"CC_SWITCH_INTERNAL_ENV_2=\"\r\n"
        );
        assert_eq!(
            carried,
            vec![(
                "CC_SWITCH_INTERNAL_ENV_2".to_string(),
                r"C:\Users\张三\.cc-switch\agy\relay".to_string()
            )]
        );
    }

    #[cfg(unix)]
    #[test]
    fn claude_terminal_config_is_private() {
        use std::os::unix::fs::PermissionsExt;
        let launch = claude_terminal_launch(
            &[("ANTHROPIC_AUTH_TOKEN".into(), "t".into())],
            "p",
            "P",
            &Default::default(),
        )
        .unwrap();
        let file = &launch.cleanup[0];
        assert_eq!(
            std::fs::metadata(file).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(
            launch.args,
            vec![
                "--settings".to_string(),
                file.to_string_lossy().into_owned()
            ]
        );
        std::fs::remove_file(file).unwrap();
    }

    #[test]
    fn claude_terminal_config_writes_the_cards_own_model_picker() {
        let read = |env: &[(&str, &str)]| {
            let env: Vec<(String, String)> = env
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();
            let launch =
                claude_terminal_launch(&env, "p", "DeepSeek", &Default::default()).unwrap();
            let file = &launch.cleanup[0];
            let value: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
            std::fs::remove_file(file).unwrap();
            value["modelPicker"].clone()
        };

        // 卡上显式配了档位：只列这几档，`model` 写档位别名。
        let picker = read(&[
            ("ANTHROPIC_MODEL", "deepseek-v4-pro"),
            ("ANTHROPIC_DEFAULT_OPUS_MODEL", "deepseek-v4-pro[1M]"),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL", "deepseek-v4-flash"),
            ("ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME", "Flash"),
        ]);
        assert_eq!(
            picker,
            serde_json::json!({
                "replaceBuiltInOptions": true,
                "options": [
                    {
                        "model": "opus",
                        "label": "Opus · deepseek-v4-pro",
                        "description": "DeepSeek · deepseek-v4-pro · 1M",
                    },
                    {
                        "model": "haiku",
                        "label": "Haiku · Flash",
                        "description": "DeepSeek · deepseek-v4-flash",
                    },
                ],
            })
        );

        // 一档都没配：空菜单，挡住用户级菜单，显示内置的。
        let picker = read(&[("ANTHROPIC_MODEL", "deepseek-v4-pro")]);
        assert_eq!(
            picker,
            serde_json::json!({ "replaceBuiltInOptions": false, "options": [] })
        );
    }

    #[test]
    fn claude_terminal_config_shadows_what_the_switch_clears() {
        let drops = crate::services::provider::SwitchDrops {
            env: serde_json::json!({
                "ANTHROPIC_AUTH_TOKEN": "PROXY_MANAGED",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "ccs-claude-kimi--k3[1M]",
            })
            .as_object()
            .unwrap()
            .clone(),
            top: serde_json::json!({
                "model": "ccs-claude-kimi--k3[1M]",
                "modelOverrides": { "a": "b" },
                "modelPicker": { "replaceBuiltInOptions": true, "options": [] },
            })
            .as_object()
            .unwrap()
            .clone(),
        };
        let launch = claude_terminal_launch(
            &[
                ("ANTHROPIC_API_KEY".into(), "sk".into()),
                (
                    "ANTHROPIC_DEFAULT_OPUS_MODEL".into(),
                    "deepseek-v4-pro".into(),
                ),
            ],
            "p",
            "DeepSeek",
            &drops,
        )
        .unwrap();
        let file = &launch.cleanup[0];
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        std::fs::remove_file(file).unwrap();

        // 卡上有的照写，没有的写空值挡住用户级的。
        assert_eq!(value["env"]["ANTHROPIC_API_KEY"], "sk");
        assert_eq!(
            value["env"]["ANTHROPIC_DEFAULT_OPUS_MODEL"],
            "deepseek-v4-pro"
        );
        assert_eq!(value["env"]["ANTHROPIC_AUTH_TOKEN"], "");
        assert_eq!(value["model"], "");
        assert_eq!(value["modelOverrides"], serde_json::json!({}));
        // 菜单写卡自己的，不是空对象。
        assert_eq!(value["modelPicker"]["options"][0]["model"], "opus");
    }

    #[test]
    fn claude_terminal_config_without_drops_writes_only_the_card() {
        let launch = claude_terminal_launch(
            &[("ANTHROPIC_MODEL".into(), "deepseek-v4-pro".into())],
            "p",
            "DeepSeek",
            &Default::default(),
        )
        .unwrap();
        let file = &launch.cleanup[0];
        let value: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(file).unwrap()).unwrap();
        std::fs::remove_file(file).unwrap();
        let keys: Vec<&String> = value.as_object().unwrap().keys().collect();
        assert_eq!(keys, ["env", "modelPicker"]);
        assert_eq!(
            value["modelPicker"],
            serde_json::json!({ "replaceBuiltInOptions": false, "options": [] })
        );
    }

    #[test]
    fn test_build_final_shell_cd_command() {
        assert_eq!(build_final_shell_cd_command("/bin/zsh", None), "");
        assert_eq!(
            build_final_shell_cd_command("/bin/zsh", Some(Path::new("/tmp/project"))),
            ""
        );
        assert_eq!(
            build_final_shell_cd_command("/bin/bash", Some(Path::new("/tmp/project O'Brien"))),
            "cd '/tmp/project O'\"'\"'Brien' || exit 1\n"
        );
    }

    #[cfg(unix)]
    #[test]
    fn test_get_user_shell_fallback() {
        // $SHELL 未设置时应按平台 fallback
        // 此测试验证 fallback 逻辑，但不验证环境变量值（取决于运行环境）
        let shell = get_user_shell();
        // 至少应返回一个合法的绝对路径
        assert!(valid_user_shell_path(&shell));
        // basename 应为合法 shell 名
        let basename = shell.rsplit('/').next().unwrap_or("sh");
        assert!(["sh", "bash", "zsh", "fish", "dash"].contains(&basename));
    }

    #[cfg(unix)]
    #[test]
    fn test_valid_user_shell_path() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let executable_zsh = temp.path().join("zsh");
        std::fs::write(&executable_zsh, "#!/usr/bin/env sh\n")
            .expect("shell fixture should be written");
        set_test_executable(&executable_zsh, true);

        let executable_fish_dir = temp.path().join("homebrew dir/bin");
        std::fs::create_dir_all(&executable_fish_dir)
            .expect("shell fixture directory should be created");
        let executable_fish = executable_fish_dir.join("fish");
        std::fs::write(&executable_fish, "#!/usr/bin/env sh\n")
            .expect("shell fixture should be written");
        set_test_executable(&executable_fish, true);

        let non_executable_bash = temp.path().join("bash");
        std::fs::write(&non_executable_bash, "#!/usr/bin/env sh\n")
            .expect("shell fixture should be written");
        set_test_executable(&non_executable_bash, false);

        assert!(valid_user_shell_path(&executable_zsh.to_string_lossy()));
        assert!(valid_user_shell_path(&executable_fish.to_string_lossy()));
        assert!(!valid_user_shell_path(""));
        assert!(!valid_user_shell_path("zsh"));
        assert!(!valid_user_shell_path(
            &temp.path().join("missing/zsh").to_string_lossy()
        ));
        assert!(!valid_user_shell_path(
            &non_executable_bash.to_string_lossy()
        ));
        assert!(!valid_user_shell_path(
            &temp.path().join("zsh; rm -rf /").to_string_lossy()
        ));
        assert!(!valid_user_shell_path(&format!(
            "{}\n/bin/bash",
            executable_zsh.to_string_lossy()
        )));
        assert!(!valid_user_shell_path("/usr/bin/powershell"));
    }

    #[test]
    fn version_probe_sentinel_drops_shell_startup_output() {
        assert_eq!(
            version_probe_payload("claude"),
            "echo __CCSWITCH_VERSION__; claude --version"
        );

        // Ubuntu 的 update-motd 在当天第一个交互式 login shell 里打印 MOTD，
        // 整段取第一个版本号会拿到 24.04.4（#7347）
        let motd = "Welcome to Ubuntu 24.04.4 LTS (GNU/Linux 6.6.87.2-microsoft-standard-WSL2 x86_64)\n\n * Documentation:  https://help.ubuntu.com\n__CCSWITCH_VERSION__\n2.1.270 (Claude Code)";
        assert_eq!(after_version_sentinel(motd), "2.1.270 (Claude Code)");

        // 兜底链 `-lic || -lc || -c` 会多次打印哨兵：取最后一次之后
        let chained = "__CCSWITCH_VERSION__\nbash: warning\n__CCSWITCH_VERSION__\n1.2.3";
        assert_eq!(after_version_sentinel(chained), "1.2.3");

        // OSC 序列没有换行、直接粘在哨兵前面（地址取自 RFC 5737 文档保留段）
        let glued = "\x1b]1337;RemoteHost=user@198.51.100.23\x07__CCSWITCH_VERSION__\n0.154.0";
        assert_eq!(after_version_sentinel(glued), "0.154.0");

        // 版本打在 stderr 的工具：哨兵之后为空，调用方回退到 stderr
        assert_eq!(after_version_sentinel("__CCSWITCH_VERSION__\n"), "");

        // 没有哨兵（shell 在执行载荷前就失败）：原样返回，行为不变
        assert_eq!(
            after_version_sentinel("sh: 1: bad: not found"),
            "sh: 1: bad: not found"
        );
    }

    #[test]
    fn test_extract_version() {
        assert_eq!(extract_version("claude 1.0.20"), "1.0.20");
        assert_eq!(extract_version("v2.3.4-beta.1"), "2.3.4-beta.1");
        // Hermes main/canary 开发构建:保留 local version 段,前端据此识别、不误报升级
        assert_eq!(
            extract_version("Hermes Agent v0.22.1+238.g8211bb9 (2026.9.24) · upstream 8211bb96"),
            "0.22.1+238.g8211bb9"
        );
        assert_eq!(extract_version("no version here"), "no version here");
    }

    #[test]
    fn local_versions_are_dev_builds_and_incomparable() {
        // 开发构建(git describe / 开发通道标识)与任何版本不可比
        assert!(parse_semver("0.22.1+238.g8211bb9").is_none());
        assert!(parse_semver("0.22.1+canary.20261015T070051Z").is_none());
        assert!(compare_semver("0.22.2", "0.22.1+238.g8211bb9").is_none());
        assert!(compare_semver("0.22.1+238.g8211bb9", "0.22.1").is_none());
        // 普通 semver build metadata 按规范忽略后照常比较
        assert_eq!(parse_semver("1.0.0+linux.1"), Some(([1, 0, 0], vec![])));
        assert_eq!(
            compare_semver("1.0.1", "1.0.0+linux.1"),
            Some(std::cmp::Ordering::Greater)
        );
        // 正式版与预发布版不受影响
        assert_eq!(
            compare_semver("0.22.2", "0.22.1"),
            Some(std::cmp::Ordering::Greater)
        );
        assert_eq!(
            compare_semver("2.1.156-beta.1", "2.1.156"),
            Some(std::cmp::Ordering::Less)
        );
    }

    #[test]
    fn github_release_version_prefers_semver_in_name_over_calendar_tag() {
        // Hermes 官方 release：tag 日历式，语义版本只在 name 里；2026-08-19 起括号内还多了个 v
        for (name, tag, want) in [
            ("Hermes Agent v0.20.4 (2026.8.18)", "v2026.8.18", "0.20.4"),
            ("Hermes Agent v0.21.0 (v2026.8.31)", "v2026.8.31", "0.21.0"),
            (
                "Hermes Agent v0.20.3 (2026.8.16.2)",
                "v2026.8.16.2",
                "0.20.3",
            ),
        ] {
            let json = serde_json::json!({ "name": name, "tag_name": tag });
            assert_eq!(
                github_release_version_from_json(&json).as_deref(),
                Some(want),
                "{name}"
            );
        }
    }

    #[test]
    fn github_release_version_falls_back_to_semver_tag() {
        // opencode：name == tag，走 name 或 tag 结果一致
        let named = serde_json::json!({ "name": "v1.18.18", "tag_name": "v1.18.18" });
        assert_eq!(
            github_release_version_from_json(&named).as_deref(),
            Some("1.18.18")
        );
        let prose = serde_json::json!({ "name": "August refresh", "tag_name": "v1.18.18" });
        assert_eq!(
            github_release_version_from_json(&prose).as_deref(),
            Some("1.18.18")
        );
        let unnamed = serde_json::json!({ "tag_name": "v1.2.3" });
        assert_eq!(
            github_release_version_from_json(&unnamed).as_deref(),
            Some("1.2.3")
        );
    }

    #[test]
    fn github_release_version_rejects_calendar_versions_and_rate_limit_body() {
        // name 与 tag 都只有日历式数字：不得把 2026.8.31 当版本号（前端会永久判定"可更新"）
        let calendar = serde_json::json!({
            "name": "Hermes Agent (2026.8.31)",
            "tag_name": "v2026.8.31"
        });
        assert_eq!(github_release_version_from_json(&calendar), None);
        let four_seg = serde_json::json!({ "tag_name": "v2026.8.16.2" });
        assert_eq!(github_release_version_from_json(&four_seg), None);
        // GitHub 未认证限流响应只有 message / documentation_url
        let limited = serde_json::json!({
            "message": "API rate limit exceeded for 1.2.3.4.",
            "documentation_url": "https://docs.github.com/rest"
        });
        assert_eq!(github_release_version_from_json(&limited), None);
        assert_eq!(
            github_release_version_from_json(&serde_json::json!({})),
            None
        );
    }

    #[test]
    fn hermes_pypi_fallback_is_hidden_when_local_leads() {
        let pypi = || Some("0.19.0".to_string());
        // GitHub 不可达、PyPI 仍停在 0.19.0：本地 0.21.0 时不展示旧值（否则"最新 < 当前"）
        assert_eq!(drop_latest_behind_local(pypi(), Some("0.21.0")), None);
        // 本地等于 / 落后 PyPI，或本地未知：照常展示
        assert_eq!(
            drop_latest_behind_local(pypi(), Some("0.19.0")).as_deref(),
            Some("0.19.0")
        );
        assert_eq!(
            drop_latest_behind_local(pypi(), Some("0.18.2")).as_deref(),
            Some("0.19.0")
        );
        assert_eq!(
            drop_latest_behind_local(pypi(), None).as_deref(),
            Some("0.19.0")
        );
        // 本地无法解析时保守视为未领先
        assert_eq!(
            drop_latest_behind_local(pypi(), Some("unknown")).as_deref(),
            Some("0.19.0")
        );
        assert_eq!(drop_latest_behind_local(None, Some("0.21.0")), None);
    }

    #[test]
    fn grok_lifecycle_metadata_is_consistent() {
        let requested = vec!["unsupported".to_string(), "grok".to_string()];
        assert_eq!(normalize_requested_tools(&requested), vec!["grok"]);
        assert_eq!(tool_display_name("grok"), "Grok Build");
        assert_eq!(npm_package_for("grok"), Some("@xai-official/grok"));
        assert_eq!(
            npm_install_command_for("grok"),
            Some(
                "npm i -g @xai-official/grok@latest --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok"
            )
        );
        assert_eq!(official_update_args("grok"), Some("update"));

        for action in [ToolLifecycleAction::Install, ToolLifecycleAction::Update] {
            assert_eq!(
                tool_action_shell_command_for_shell("grok", action, LifecycleCommandShell::Posix)
                    .as_deref(),
                npm_install_command_for("grok")
            );
        }

        // Static update remains package-manager based. Only a path positively
        // identified as xAI's native install may run `grok update`.
        assert_eq!(
            tool_action_shell_command_for_shell(
                "grok",
                ToolLifecycleAction::Update,
                LifecycleCommandShell::WindowsBatch,
            )
            .as_deref(),
            npm_install_command_for("grok")
        );
    }

    #[test]
    fn dsh_lifecycle_uses_npm_and_keeps_engine_checks_for_anchored_updates() {
        let expected = "npm i -g @deepseek-ai/dsh@latest --engine-strict";
        assert_eq!(posix_install_command_for("dsh"), expected);
        assert_eq!(npm_package_for("dsh"), Some("@deepseek-ai/dsh"));
        for action in [ToolLifecycleAction::Install, ToolLifecycleAction::Update] {
            assert_eq!(
                tool_action_shell_command_for_shell("dsh", action, LifecycleCommandShell::Posix)
                    .as_deref(),
                Some(expected)
            );
            assert_eq!(
                tool_action_shell_command_for_shell(
                    "dsh",
                    action,
                    LifecycleCommandShell::WindowsBatch
                )
                .as_deref(),
                Some(expected)
            );
        }
        assert_eq!(official_update_args("dsh"), None);
        assert_eq!(npm_install_extra_args("dsh"), " --engine-strict");
    }

    #[test]
    fn kimi_install_uses_bundled_runtime_instead_of_system_node() {
        let posix = tool_action_shell_command_for_shell(
            "kimi",
            ToolLifecycleAction::Install,
            LifecycleCommandShell::Posix,
        )
        .unwrap();
        assert_eq!(posix, KIMI_INSTALL_UNIX);
        assert_eq!(posix_install_command_for("kimi"), posix);
        assert!(!posix.contains("npm"));
        let update = tool_action_shell_command_for_shell(
            "kimi",
            ToolLifecycleAction::Update,
            LifecycleCommandShell::Posix,
        )
        .unwrap();
        assert_eq!(update, format!("kimi upgrade --yes || {posix}"));
        assert!(!update.contains("npm"));
    }

    #[test]
    fn kimi_native_install_root_follows_the_installer_layout() {
        assert_eq!(
            kimi_native_install_root(
                "/home/me/.kimi-code/bin/kimi",
                "/home/me/.kimi-code/bin/kimi"
            )
            .as_deref(),
            Some("/home/me/.kimi-code")
        );
        // 软链指回安装目录时写回真身那处。
        assert_eq!(
            kimi_native_install_root("/home/me/.local/bin/kimi", "/opt/kimi/bin/kimi").as_deref(),
            Some("/opt/kimi")
        );
        assert_eq!(
            kimi_native_install_root(
                r"C:\Users\sc\.kimi-code\bin\kimi.exe",
                r"\\?\C:\Users\sc\.kimi-code\bin\kimi.exe"
            )
            .as_deref(),
            Some(r"C:\Users\sc\.kimi-code")
        );
        // npm 版不归 installer 管。
        assert_eq!(
            kimi_native_install_root(
                "/home/me/.nvm/versions/node/v22.19.0/bin/kimi",
                "/home/me/.nvm/versions/node/v22.19.0/lib/node_modules/@moonshot-ai/kimi-code/dist/main.mjs"
            ),
            None
        );
        assert_eq!(
            kimi_native_install_root(
                r"C:\Users\sc\AppData\Roaming\npm\kimi.cmd",
                r"\\?\C:\Users\sc\AppData\Roaming\npm\kimi.cmd"
            ),
            None
        );
    }

    #[test]
    fn legacy_kimi_native_update_reruns_the_installer() {
        let install = |path: &str, version: &str| ToolInstallation {
            path: path.to_string(),
            version: Some(version.to_string()),
            runnable: true,
            error: None,
            exited_with_error: false,
            source: infer_install_source(Path::new(path)).to_string(),
            is_path_default: true,
            real: PathBuf::from(path),
        };
        let (path, root) = if cfg!(target_os = "windows") {
            (
                r"C:\Users\sc\.kimi-code\bin\kimi.exe",
                r"C:\Users\sc\.kimi-code",
            )
        } else {
            ("/home/me/.kimi-code/bin/kimi", "/home/me/.kimi-code")
        };

        // 0.43.0 之前 `upgrade --yes` 在非 TTY 下只打印提示就 exit 0（#9032）。
        assert_eq!(
            installs_anchored_command("kimi", &[install(path, "0.23.1")]),
            Some(kimi_installer_update_command(root))
        );
        // 新版照旧走 CLI 自升级。
        let current = installs_anchored_command("kimi", &[install(path, "2.4.0")]).unwrap();
        assert!(current.contains("upgrade --yes"), "{current}");
        assert_eq!(
            installs_anchored_command("kimi", &[install(path, "0.43.0")]),
            Some(current)
        );
        // Volta 的 shim 也是 `bin/kimi`，必须交给 Volta，不能让 installer 覆盖。
        #[cfg(not(target_os = "windows"))]
        {
            let volta =
                installs_anchored_command("kimi", &[install("/home/me/.volta/bin/kimi", "0.23.1")])
                    .unwrap();
            assert!(
                volta.ends_with("volta install @moonshot-ai/kimi-code"),
                "{volta}"
            );
        }
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn kimi_installer_update_command_targets_the_detected_dir() {
        assert_eq!(
            kimi_installer_update_command("/home/me/my kimi"),
            format!("KIMI_INSTALL_DIR='/home/me/my kimi' {KIMI_INSTALL_UNIX}")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn kimi_windows_install_is_noninteractive_and_has_no_npm_fallback() {
        use base64::Engine as _;
        let command = tool_action_shell_command_for_shell(
            "kimi",
            ToolLifecycleAction::Install,
            LifecycleCommandShell::WindowsBatch,
        )
        .unwrap();
        assert!(command.starts_with("powershell -NoProfile -NonInteractive "));
        let encoded = command.split("-EncodedCommand ").nth(1).unwrap();
        let bytes = base64::engine::general_purpose::STANDARD
            .decode(encoded)
            .unwrap();
        let script = String::from_utf16(
            &bytes
                .chunks_exact(2)
                .map(|unit| u16::from_le_bytes([unit[0], unit[1]]))
                .collect::<Vec<_>>(),
        )
        .unwrap();
        assert_eq!(script, KIMI_INSTALL_WINDOWS_SCRIPT);
        assert!(!command.contains("npm"));
        assert_eq!(
            wsl_tool_action_shell_command("kimi", ToolLifecycleAction::Install).as_deref(),
            Some(KIMI_INSTALL_UNIX)
        );
    }

    #[test]
    fn pi_lifecycle_metadata_matches_pinned_distribution() {
        let requested = vec!["unsupported".to_string(), "pi".to_string()];
        assert_eq!(normalize_requested_tools(&requested), vec!["pi"]);
        assert_eq!(tool_display_name("pi"), "Pi");
        assert_eq!(
            npm_package_for("pi"),
            Some("@earendil-works/pi-coding-agent")
        );
        assert_eq!(
            npm_install_command_for("pi"),
            Some("npm i -g @earendil-works/pi-coding-agent@latest")
        );
        // The verified distribution exposes `pi --version`, but no updater
        // contract is assumed; upgrades stay on the package-manager path.
        assert_eq!(official_update_args("pi"), None);
    }

    #[test]
    fn omp_lifecycle_metadata_names_the_tool_and_keeps_one_package() {
        let requested = vec!["unsupported".to_string(), "omp".to_string()];
        assert_eq!(normalize_requested_tools(&requested), vec!["omp"]);
        // Without this arm the user-facing strings read "Unknown at
        // <path> was not installed by npm..." and the batch header prints
        // `========== Unknown ==========`.
        assert_eq!(tool_display_name("omp"), "Oh My Pi");
        // The version check reads this package; npm cannot run it (the entry is
        // a bun script), so there is no npm install command at all.
        assert_eq!(npm_package_for("omp"), Some("@oh-my-pi/pi-coding-agent"));
        assert_eq!(npm_install_command_for("omp"), None);
        // `omp update` routes every install source itself and never prompts.
        assert_eq!(official_update_args("omp"), Some("update"));
        #[cfg(not(target_os = "windows"))]
        {
            assert_eq!(install_command_for("omp"), OMP_INSTALL_UNIX);
            assert_eq!(
                tool_action_shell_command_for_shell(
                    "omp",
                    ToolLifecycleAction::Update,
                    LifecycleCommandShell::Posix
                )
                .as_deref(),
                Some(format!("omp update || {OMP_INSTALL_UNIX}").as_str())
            );
        }
    }

    #[test]
    fn mcode_lifecycle_metadata_skips_non_tty_self_update() {
        let requested = vec!["unsupported".to_string(), "mcode".to_string()];
        assert_eq!(normalize_requested_tools(&requested), vec!["mcode"]);
        assert_eq!(tool_display_name("mcode"), "MiniMax Code");
        // npm 上同名的 `mcode` 是无关项目，官方包是 scoped 的 @minimax-ai/code。
        assert_eq!(npm_package_for("mcode"), Some("@minimax-ai/code"));
        let npm = npm_install_command_for("mcode").unwrap();
        assert_eq!(
            npm,
            format!(
                "npm i -g @minimax-ai/code@latest{}",
                npm_install_extra_args("mcode")
            )
        );
        assert!(
            npm.contains("\"--allow-scripts=@minimax-ai/code,better-sqlite3\""),
            "npm 12 blocks better-sqlite3's install script without it: {npm}"
        );
        assert_eq!(npm_install_extra_args("codex"), "");

        // `mcode update` exits 0 without installing when stdin/stdout are not a
        // TTY, which is exactly how silent lifecycle actions run it.
        assert_eq!(official_update_args("mcode"), None);
        for shell in [
            LifecycleCommandShell::Posix,
            LifecycleCommandShell::WindowsBatch,
        ] {
            assert_eq!(
                tool_action_shell_command_for_shell("mcode", ToolLifecycleAction::Update, shell)
                    .as_deref(),
                Some(npm)
            );
        }
    }

    #[test]
    fn mcode_install_root_candidate_follows_platform_layout() {
        assert_eq!(
            mcode_install_root_candidate("/opt/mcode/bin/mcode", McodeLayout::Posix),
            "/opt/mcode"
        );
        // 根目录本身叫 bin：POSIX 只去掉入口所在的那一层 bin。
        assert_eq!(
            mcode_install_root_candidate("/opt/bin/bin/mcode", McodeLayout::Posix),
            "/opt/bin"
        );
        // Windows 入口直接在根目录，根目录叫 bin 也不能再上跳一级。
        assert_eq!(
            mcode_install_root_candidate("C:\\Users\\me\\bin\\mcode.cmd", McodeLayout::Windows),
            "C:\\Users\\me\\bin"
        );
    }

    #[test]
    fn mcode_script_install_root_matches_only_installer_layout() {
        assert_eq!(
            mcode_script_install_root(
                "/Users/me/.minimax-code/bin/mcode",
                "/Users/me/.minimax-code/bin/mcode",
                McodeLayout::Posix,
            )
            .as_deref(),
            Some("/Users/me/.minimax-code")
        );
        assert_eq!(
            mcode_script_install_root(
                "C:\\Users\\me\\.minimax-code\\mcode.cmd",
                "C:\\Users\\me\\.minimax-code\\mcode.cmd",
                McodeLayout::Windows,
            )
            .as_deref(),
            Some("C:\\Users\\me\\.minimax-code")
        );
        // 用户自建软链：入口不在安装目录，但真身是 launcher。
        assert_eq!(
            mcode_script_install_root(
                "/Users/me/.local/bin/mcode",
                "/Users/me/.minimax-code/bin/mcode",
                McodeLayout::Posix,
            )
            .as_deref(),
            Some("/Users/me/.minimax-code")
        );
        // 全局 npm 安装、以及同名数据目录 `~/.minimax` 都不是 installer 布局。
        assert_eq!(
            mcode_script_install_root(
                "/opt/homebrew/bin/mcode",
                "/opt/homebrew/lib/node_modules/@minimax-ai/code/cli.js",
                McodeLayout::Posix,
            ),
            None
        );
        assert_eq!(
            mcode_script_install_root(
                "/Users/me/.minimax/bin/mcode",
                "/Users/me/.minimax/bin/mcode",
                McodeLayout::Posix,
            ),
            None
        );
    }

    /// 在 tempdir 下按 `layout` 搭一个自定义 `MCODE_INSTALL_DIR`（`<tmp>/apps/<root_name>`，
    /// 目录名不是 `.minimax-code`）的 installer 布局，返回 `(TempDir, root, 入口路径)`。
    fn mcode_custom_install(
        root_name: &str,
        layout: McodeLayout,
    ) -> (tempfile::TempDir, String, String) {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("apps").join(root_name);
        let entry = match layout {
            McodeLayout::Posix => root.join("bin").join("mcode"),
            McodeLayout::Windows => root.join("mcode.cmd"),
        };
        std::fs::create_dir_all(entry.parent().unwrap()).unwrap();
        std::fs::write(&entry, "").unwrap();
        (
            dir,
            root.to_string_lossy().to_string(),
            entry.to_string_lossy().to_string(),
        )
    }

    fn write_mcode_receipt(root: &str, product: &str, owner: &str, prefix: &str, bom: bool) {
        let json = serde_json::json!({
            "schemaVersion": 2,
            "product": product,
            "updateOwner": owner,
            "prefix": prefix,
        })
        .to_string();
        let text = if bom { format!("\u{feff}{json}") } else { json };
        std::fs::write(Path::new(root).join("install.json"), text).unwrap();
    }

    #[test]
    fn mcode_receipt_claims_custom_install_dir() {
        for (root_name, layout) in [
            ("mcode", McodeLayout::Posix),
            ("bin", McodeLayout::Posix),
            ("mcode", McodeLayout::Windows),
            // MCODE_INSTALL_DIR=C:\Users\me\bin：入口 `<root>\mcode.cmd` 的父目录名就是 bin。
            ("bin", McodeLayout::Windows),
        ] {
            let (_dir, root, entry) = mcode_custom_install(root_name, layout);
            write_mcode_receipt(&root, "minimax-code", "npm-prefix", &root, false);
            assert_eq!(
                mcode_receipt_install_root(&entry, layout).as_deref(),
                Some(root.as_str()),
                "{root_name} / {layout:?}"
            );
        }

        // 端到端：单处安装也必须原地重跑 installer，不能退到全局 `npm i -g`。
        let (_dir, root, entry) = mcode_custom_install("mcode", McodeLayout::NATIVE);
        write_mcode_receipt(&root, "minimax-code", "npm-prefix", &root, false);
        let installs = vec![ToolInstallation {
            path: entry.clone(),
            version: Some("0.5.3".to_string()),
            runnable: true,
            error: None,
            exited_with_error: false,
            source: infer_install_source(Path::new(&entry)).to_string(),
            is_path_default: false,
            real: PathBuf::from(&entry),
        }];
        assert_eq!(
            installs_anchored_command("mcode", &installs),
            Some(mcode_installer_update_command(&root))
        );
    }

    #[test]
    fn mcode_receipt_tolerates_utf8_bom() {
        // install.ps1 在 Windows PowerShell 5.1 下用 `Set-Content -Encoding UTF8` 写回执，带 BOM。
        let (_dir, root, entry) = mcode_custom_install("mcode", McodeLayout::Windows);
        write_mcode_receipt(&root, "minimax-code", "npm-prefix", &root, true);
        assert_eq!(
            mcode_receipt_install_root(&entry, McodeLayout::Windows).as_deref(),
            Some(root.as_str())
        );
    }

    #[test]
    fn mcode_receipt_rejects_foreign_or_mismatched_receipts() {
        let layout = McodeLayout::NATIVE;
        // 没有回执：不认领。
        let (_dir, _root, entry) = mcode_custom_install("mcode", layout);
        assert_eq!(mcode_receipt_install_root(&entry, layout), None);

        // 不是 MiniMax Code 的回执、或不归 npm-prefix 管：不认领。
        for (product, owner) in [
            ("other", "npm-prefix"),
            ("minimax-code", "managed-installer"),
        ] {
            let (_dir, root, entry) = mcode_custom_install("mcode", layout);
            write_mcode_receipt(&root, product, owner, &root, false);
            assert_eq!(
                mcode_receipt_install_root(&entry, layout),
                None,
                "{product}/{owner}"
            );
        }

        // prefix 指向别的目录（拷贝来的回执）：不认领。
        let elsewhere = tempfile::tempdir().unwrap();
        let (_dir, root, entry) = mcode_custom_install("mcode", layout);
        write_mcode_receipt(
            &root,
            "minimax-code",
            "npm-prefix",
            &elsewhere.path().to_string_lossy(),
            false,
        );
        assert_eq!(mcode_receipt_install_root(&entry, layout), None);
    }

    #[test]
    fn test_compare_semver() {
        use std::cmp::Ordering;
        assert_eq!(
            compare_semver("2.1.156", "2.1.154"),
            Some(Ordering::Greater)
        );
        assert_eq!(compare_semver("2.1.154", "2.1.156"), Some(Ordering::Less));
        assert_eq!(compare_semver("2.1.156", "2.1.156"), Some(Ordering::Equal));
        // 预发布 < 同核心正式版
        assert_eq!(
            compare_semver("2.1.156-beta.1", "2.1.156"),
            Some(Ordering::Less)
        );
        // core 更高的预发布仍高于较低的正式版（gemini nightly 场景）
        assert_eq!(
            compare_semver("0.45.0-nightly.1", "0.44.1"),
            Some(Ordering::Greater)
        );
        // 大 patch（codex 时间戳式）不溢出
        assert_eq!(
            compare_semver("0.1.2505172116", "0.135.0"),
            Some(Ordering::Less)
        );
        // 无法解析返回 None（gemini 的 `false` 脏 tag）
        assert_eq!(compare_semver("false", "1.0.0"), None);
    }

    #[test]
    fn test_pick_latest_version() {
        use serde_json::json;
        let tags = json!({
            "latest": "2.1.154",
            "next": "2.1.156",
            "stable": "2.1.145"
        });
        let map = tags.as_object().unwrap();

        // 本地领先 latest（在 next 通道）→ 补查到 next，数字对齐
        assert_eq!(
            pick_latest_version(map, &["next"], Some("2.1.156")),
            Some("2.1.156".to_string())
        );
        // 本地等于 latest → 不补查，仍显示 latest
        assert_eq!(
            pick_latest_version(map, &["next"], Some("2.1.154")),
            Some("2.1.154".to_string())
        );
        // 本地落后 latest（稳定通道用户）→ 不补查，不被推向预发布版
        assert_eq!(
            pick_latest_version(map, &["next"], Some("2.1.145")),
            Some("2.1.154".to_string())
        );
        // 无预发布白名单 → 永远只看 latest（不解析 local，避免脏 local 触发）
        assert_eq!(
            pick_latest_version(map, &[], Some("2.1.156")),
            Some("2.1.154".to_string())
        );
        // 本地版本未知 → 保守只看 latest
        assert_eq!(
            pick_latest_version(map, &["next"], None),
            Some("2.1.154".to_string())
        );
    }

    #[test]
    fn test_pick_latest_version_filters_dirty_prerelease() {
        use serde_json::json;
        // 模拟 codex：beta 是低于 latest 的时间戳式脏版本
        let tags = json!({
            "latest": "0.135.0",
            "beta": "0.1.2505172116"
        });
        let map = tags.as_object().unwrap();
        // 即便本地领先 latest，低于 latest 的脏 beta 也不会被选
        assert_eq!(
            pick_latest_version(map, &["beta"], Some("0.200.0")),
            Some("0.135.0".to_string())
        );
    }

    #[test]
    fn test_npm_dist_tags_url() {
        // 普通包名直接拼进路径
        assert_eq!(
            npm_dist_tags_url("openclaw"),
            "https://registry.npmjs.org/-/package/openclaw/dist-tags"
        );
        // scoped 包名的 `/` 按 registry 约定转义成 %2f
        assert_eq!(
            npm_dist_tags_url("@openai/codex"),
            "https://registry.npmjs.org/-/package/@openai%2fcodex/dist-tags"
        );
    }

    /// `parent_dir` 是锚定层"由 bin 路径推导同目录绝对路径"的基石,跨平台共用——
    /// 这里固化 `\`/`/`/混合分隔符/根边界四种情况,避免未来重构悄悄改语义。
    mod parent_dir_cases {
        use super::super::*;

        #[test]
        fn unix_path() {
            assert_eq!(
                parent_dir("/Users/me/.volta/bin/codex"),
                "/Users/me/.volta/bin"
            );
        }

        #[test]
        fn windows_backslash() {
            assert_eq!(
                parent_dir("C:\\Users\\me\\AppData\\Local\\Volta\\bin\\codex.exe"),
                "C:\\Users\\me\\AppData\\Local\\Volta\\bin"
            );
        }

        #[test]
        fn mixed_separators_takes_rightmost() {
            // Windows 上 `Path::join` 与字符串拼接可能产出混合分隔符;取**两种之中最右
            // 出现**的位置,而非"优先 `\`"——后者在混合时会取错父目录。
            assert_eq!(
                parent_dir("C:\\Users\\me/Code/openclaw\\codex.cmd"),
                "C:\\Users\\me/Code/openclaw"
            );
        }

        #[test]
        fn no_separator_returns_empty() {
            // 无父目录 → 空串,锚定层据此返 None、回退静态命令。
            assert_eq!(parent_dir("codex"), "");
        }

        #[test]
        fn separator_at_root_returns_empty() {
            // `/codex`:根目录是 index 0,`i > 0` 不满足 → 空串。同款行为对 Windows
            // 上的 `\codex` 也成立(实际不会出现,但语义对齐)。
            assert_eq!(parent_dir("/codex"), "");
            assert_eq!(parent_dir("\\codex"), "");
        }
    }

    /// Codex 官方独立安装器的识别是纯字符串判定,POSIX / Windows 布局都在这里固化。
    mod codex_standalone_detection {
        use super::super::*;

        #[test]
        fn posix_default_layout() {
            assert_eq!(
                codex_standalone_install(
                    "/Users/me/.local/bin/codex",
                    "/Users/me/.codex/packages/standalone/releases/0.157.0-aarch64-apple-darwin/bin/codex",
                ),
                Some(CodexStandaloneInstall {
                    install_dir: "/Users/me/.local/bin".to_string(),
                    codex_home: Some("/Users/me/.codex".to_string()),
                })
            );
        }

        #[test]
        fn windows_verbatim_real_is_restored() {
            // canonicalize 出的 `\\?\` 前缀要剥掉再交给 installer;大小写保持原样。
            assert_eq!(
                codex_standalone_install(
                    r"C:\Users\Me\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe",
                    r"\\?\C:\Users\Me\.Codex\Packages\Standalone\releases\0.157.0-x86_64-pc-windows-msvc\bin\codex.exe",
                ),
                Some(CodexStandaloneInstall {
                    install_dir: r"C:\Users\Me\AppData\Local\Programs\OpenAI\Codex\bin".to_string(),
                    codex_home: Some(r"C:\Users\Me\.Codex".to_string()),
                })
            );
            assert_eq!(
                codex_standalone_install(
                    r"\\server\share\bin\codex.exe",
                    r"\\?\UNC\server\share\cx\packages\standalone\current\bin\codex.exe",
                )
                .and_then(|install| install.codex_home),
                Some(r"\\server\share\cx".to_string())
            );
        }

        #[test]
        fn windows_default_entry_without_resolved_real() {
            // canonicalize 失败时 real 就是入口本身:仍按默认入口目录认出,只是不传 CODEX_HOME。
            let bin = r"C:\Users\me\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe";
            assert_eq!(
                codex_standalone_install(bin, bin),
                Some(CodexStandaloneInstall {
                    install_dir: r"C:\Users\me\AppData\Local\Programs\OpenAI\Codex\bin".to_string(),
                    codex_home: None,
                })
            );
        }

        #[test]
        fn other_install_channels_are_not_standalone() {
            for (bin, real) in [
                (
                    "/Users/me/.nvm/versions/node/v22.19.0/bin/codex",
                    "/Users/me/.nvm/versions/node/v22.19.0/lib/node_modules/@openai/codex/bin/codex.js",
                ),
                (
                    "/opt/homebrew/bin/codex",
                    "/opt/homebrew/Caskroom/codex/0.157.0/bin/codex",
                ),
                // app-server 守护进程的发布包不是 PATH 上的 CLI。
                (
                    "/Users/me/.local/bin/codex",
                    "/Users/me/.codex/packages/app-server-daemon/current/bin/codex",
                ),
                (r"C:\Users\me\AppData\Roaming\npm\codex.cmd", r"C:\Users\me\AppData\Roaming\npm\codex.cmd"),
            ] {
                assert_eq!(codex_standalone_install(bin, real), None, "{bin}");
            }
        }
    }

    /// 锚定落空后的兜底:认不出渠道的原生可执行文件不再退回裸 `npm i -g`(#7650),
    /// 脚本 shim / node_modules 里的入口 / 定位不到默认那处仍保持旧的静态命令。
    mod unmanaged_native_update {
        use super::super::*;

        const MACH_O_64: [u8; 4] = [0xcf, 0xfa, 0xed, 0xfe];

        /// 在 tempdir 的 `rel` 位置写入 `bytes`,返回指向它的安装记录。TempDir 须保活。
        fn installed(
            dir: &tempfile::TempDir,
            rel: &str,
            bytes: &[u8],
            is_default: bool,
        ) -> ToolInstallation {
            let path = dir.path().join(rel);
            std::fs::create_dir_all(path.parent().unwrap()).unwrap();
            std::fs::write(&path, bytes).unwrap();
            ToolInstallation {
                path: path.to_string_lossy().to_string(),
                version: Some("1.0.0".to_string()),
                runnable: true,
                error: None,
                exited_with_error: false,
                source: infer_install_source(&path).to_string(),
                is_path_default: is_default,
                real: path,
            }
        }

        #[test]
        fn native_binary_outside_node_modules_is_unmanaged() {
            let dir = tempfile::tempdir().unwrap();
            let gemini = installed(&dir, "nix-profile/bin/gemini", &MACH_O_64, true);
            assert_eq!(
                resolve_update_command("gemini", &[gemini]),
                UpdateCommand::Unmanaged
            );
            let pi = installed(&dir, "bin/pi", b"\x7fELF\x02\x01\x01", true);
            assert_eq!(
                resolve_update_command("pi", &[pi]),
                UpdateCommand::Unmanaged
            );
        }

        #[test]
        fn script_shim_keeps_static_npm_command() {
            // asdf / nodenv 的 shim 是脚本,背后可能正是 npm 全局包,不能拒绝。
            let dir = tempfile::tempdir().unwrap();
            let shim = installed(
                &dir,
                ".asdf/shims/gemini",
                b"#!/usr/bin/env bash\nexec asdf exec gemini \"$@\"\n",
                true,
            );
            assert_eq!(
                resolve_update_command("gemini", &[shim]),
                UpdateCommand::Static(static_fallback_command("gemini"))
            );
        }

        #[test]
        fn native_binary_inside_node_modules_keeps_static_npm_command() {
            // 有的 npm 包把原生二进制放在包内,真身仍在 node_modules 下,归 npm 管。
            let dir = tempfile::tempdir().unwrap();
            let bundled = installed(
                &dir,
                "prefix/lib/node_modules/@google/gemini-cli/bin/gemini",
                &MACH_O_64,
                true,
            );
            assert_eq!(
                resolve_update_command("gemini", &[bundled]),
                UpdateCommand::Static(static_fallback_command("gemini"))
            );
        }

        #[test]
        fn unknown_default_keeps_static_command() {
            // 多处安装又定位不到默认那处:不知道命令行用的是哪份,维持旧行为(前端会弹确认)。
            let dir = tempfile::tempdir().unwrap();
            let a = installed(&dir, "a/bin/gemini", &MACH_O_64, false);
            let b = installed(&dir, "b/bin/gemini", &MACH_O_64, false);
            assert_eq!(
                resolve_update_command("gemini", &[a, b]),
                UpdateCommand::Static(static_fallback_command("gemini"))
            );
        }

        #[test]
        fn native_executable_magic() {
            let dir = tempfile::tempdir().unwrap();
            let cases: [(&str, &[u8], bool); 7] = [
                ("elf", b"\x7fELF\x02", true),
                ("macho", &MACH_O_64, true),
                ("fat", &[0xca, 0xfe, 0xba, 0xbe], true),
                ("pe.exe", b"MZ\x90\x00", true),
                ("script", b"#!/bin/sh\n", false),
                ("cmd-shim.cmd", b"@ECHO off\r\n", false),
                ("short", b"MZ", false),
            ];
            for (name, bytes, expected) in cases {
                let path = dir.path().join(name);
                std::fs::write(&path, bytes).unwrap();
                assert_eq!(is_native_executable(&path), expected, "{name}");
            }
            assert!(!is_native_executable(&dir.path().join("missing")));
        }
    }

    /// Windows-only 锚定升级回归(等价类压缩到 3 种 idiom:volta/pnpm/npm)。整块通过
    /// `cfg(target_os = "windows")` gate,在 macOS/Linux 上不参与 cargo test;Windows
    /// CI 跑全套验证。tempdir 模拟 sibling 入口存在/不存在,锁定"扩展名顺序优先级 +
    /// 含空格路径自动加双引号 + 探不到 sibling → None 退静态"三件事。
    #[cfg(target_os = "windows")]
    mod anchored_upgrade_windows {
        use super::super::*;

        /// 在 tempdir 下创建子目录 `subdir`(空字符串则用 tempdir 根),放入 `entry`
        /// 与若干 `siblings` 假文件。返回 `(TempDir, 子目录, 入口绝对路径)`——TempDir
        /// 必须保活,否则析构后 fs 文件消失、`is_file()` 失败,测试假绿。
        fn setup_sibling(
            subdir: &str,
            entry: &str,
            siblings: &[&str],
        ) -> (tempfile::TempDir, std::path::PathBuf, String) {
            let dir = tempfile::tempdir().unwrap();
            let sub = if subdir.is_empty() {
                dir.path().to_path_buf()
            } else {
                dir.path().join(subdir)
            };
            std::fs::create_dir_all(&sub).unwrap();
            std::fs::write(sub.join(entry), "").unwrap();
            for s in siblings {
                std::fs::write(sub.join(s), "").unwrap();
            }
            let bin_path = sub.join(entry).to_string_lossy().to_string();
            (dir, sub, bin_path)
        }

        /// **必须与 `win_quote_path_for_batch` 主体保持镜像**——给 anchored 测试动态算
        /// expected,让用例在 temp 根目录含空格 / `&` / `(` / `%` 等特殊字符的开发机上
        /// 也能通过(默认 Windows `%TEMP%` = `C:\Users\<user>\AppData\Local\Temp`,
        /// 用户名带空格的机器整条 path 含空格、生产代码会正确加引号、测试硬编码无引号
        /// expected 会假失败)。
        ///
        /// 镜像引入"两边必须同步"的隐性依赖——回归防护层是 `win_quote_*` 那 7 个独立
        /// 单测,它们用硬编码字面值锁住 quoting 规则本身,即便此镜像漂移也会被那一组
        /// 测试 catch;反之亦然。
        fn expect_quoted_path(p: &str) -> String {
            let escaped = p.replace('%', "%%%%");
            let needs_quote = p
                .chars()
                .any(|c| matches!(c, ' ' | '&' | '(' | ')' | '^' | ';' | '<' | '>' | '|' | ','));
            if needs_quote {
                format!("\"{escaped}\"")
            } else {
                escaped
            }
        }

        #[test]
        fn volta_windows_uses_volta_install() {
            // tempdir 路径里不含 "volta" 子串,所以在 tempdir 下手建一个 `Volta` 子目录
            // 才能让 `infer_install_source` 通过路径 normalize 后命中 `/volta/` 分支。
            // sibling 候选顺序 `[exe, cmd]`——Volta 是 Rust 写的 native binary,首选 .exe。
            // expected 通过 `expect_quoted_path` 算出,以适应 temp 根目录含特殊字符的环境。
            let (_dir, sub, bin_path) = setup_sibling("Volta", "codex.cmd", &["volta.exe"]);
            let cmd = anchored_command_from_paths("codex", &bin_path, &bin_path);
            let volta_full = format!("{}\\volta.exe", sub.to_string_lossy());
            // codex 自 5092fe51 起不在 prefers_official_update：直接锚定包管理器，无 `codex update ||` 前缀。
            let expected = format!("{} install @openai/codex", expect_quoted_path(&volta_full));
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn pnpm_windows_uses_pnpm_add() {
            // bin_path 落 `%LOCALAPPDATA%\pnpm\codex.cmd`,sibling 有 `pnpm.cmd` → 锚定到
            // `<dir>\pnpm.cmd add -g @openai/codex@latest`。用 add+@latest 而非 update,
            // 兼容"之前没通过 pnpm 装过"的幂等性场景。
            let (_dir, sub, bin_path) = setup_sibling("pnpm", "codex.cmd", &["pnpm.cmd"]);
            let cmd = anchored_command_from_paths("codex", &bin_path, &bin_path);
            let pnpm_full = format!("{}\\pnpm.cmd", sub.to_string_lossy());
            // codex 自 5092fe51 起不在 prefers_official_update：直接锚定包管理器，无 `codex update ||` 前缀。
            let expected = format!(
                "{} add -g @openai/codex@latest",
                expect_quoted_path(&pnpm_full)
            );
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn opencode_windows_uses_package_fallback_without_official_upgrade() {
            let (_dir, sub, bin_path) = setup_sibling("pnpm", "opencode.cmd", &["pnpm.cmd"]);
            let cmd = anchored_command_from_paths("opencode", &bin_path, &bin_path);
            let pnpm_full = format!("{}\\pnpm.cmd", sub.to_string_lossy());
            let expected = format!(
                "{} add -g opencode-ai@latest",
                expect_quoted_path(&pnpm_full)
            );
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn opencode_windows_static_fallback_skips_official_upgrade() {
            let cmd = static_fallback_command("opencode");
            assert_eq!(cmd, npm_install_command_for("opencode").unwrap());
            assert!(!cmd.contains("opencode upgrade"));
        }

        #[test]
        fn npm_windows_default_branch() {
            // 任意 system 类路径(不命中 volta/pnpm)→ 兜底 sibling npm.cmd 锚定。
            // 模拟 nvm-windows 的实际形态:`<NVM_HOME>\v22.0.0\codex.cmd`。
            let (_dir, sub, bin_path) = setup_sibling("v22.0.0", "codex.cmd", &["npm.cmd"]);
            let cmd = anchored_command_from_paths("codex", &bin_path, &bin_path);
            let npm_full = format!("{}\\npm.cmd", sub.to_string_lossy());
            // codex 自 5092fe51 起不在 prefers_official_update：直接锚定包管理器，无 `codex update ||` 前缀。
            let expected = format!(
                "{} i -g @openai/codex@latest",
                expect_quoted_path(&npm_full)
            );
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn grok_windows_anchors_to_sibling_npm() {
            let (_dir, sub, bin_path) = setup_sibling("v22.0.0", "grok.cmd", &["npm.cmd"]);
            let cmd = anchored_command_from_paths("grok", &bin_path, &bin_path);
            let npm_full = format!("{}\\npm.cmd", sub.to_string_lossy());
            let expected = format!(
                "{} i -g @xai-official/grok@latest{}",
                expect_quoted_path(&npm_full),
                npm_install_extra_args("grok")
            );
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn grok_native_windows_uses_self_update_with_installer_fallback() {
            // sibling 有 npm.cmd 也**不能**拿它当 fallback:`grok update` 本身就是靠 npm
            // 分发的(见 grok_native_update_command doc),npm fallback 与 primary 同源、
            // 会一起失败。fallback 必须是官方 PowerShell installer —— 唯一不经 npm 的路径。
            let (_dir, _sub, bin_path) = setup_sibling(".grok/bin", "grok.exe", &["npm.cmd"]);
            let cmd = anchored_command_from_paths("grok", &bin_path, &bin_path).unwrap();
            let expected = format!(
                "{} update || {}",
                expect_quoted_path(&bin_path),
                grok_install_windows_command()
            );
            assert_eq!(cmd, expected);
            // fallback 是 powershell.exe 不是 .cmd/.bat —— `||` 右侧不该有 `call`。
            assert!(
                !cmd.contains("|| call"),
                "powershell needs no `call`: {cmd}"
            );
            assert!(!cmd.contains("npm"), "npm must not be the fallback: {cmd}");
        }

        #[test]
        fn mcode_script_install_windows_reruns_installer_in_place() {
            // sibling 有 npm.cmd 也不能用：脚本安装不归全局 npm 管，npm 会装到另一处。
            let (_dir, sub, bin_path) =
                setup_sibling(".minimax-code", "mcode.cmd", &["mcode.ps1", "npm.cmd"]);
            let cmd = anchored_command_from_paths("mcode", &bin_path, &bin_path).unwrap();
            assert_eq!(cmd, mcode_installer_update_command(&sub.to_string_lossy()));
            assert!(!cmd.contains("npm"), "npm must not be used: {cmd}");
        }

        #[test]
        fn mcode_windows_installer_update_pins_install_root() {
            let cmd = mcode_installer_update_command("C:\\Users\\o'brien\\.minimax-code");
            let expected = powershell_encoded_command(
                "$env:MCODE_INSTALL_DIR = 'C:\\Users\\o''brien\\.minimax-code'; irm https://filecdn.minimax.chat/public/install.ps1 | iex",
            );
            assert_eq!(
                cmd.split_once("-EncodedCommand ")
                    .map(|(_, encoded)| encoded),
                Some(expected.as_str())
            );
        }

        #[test]
        fn codex_standalone_windows_reruns_installer_in_place() {
            // #7650:独立安装的 codex.exe 旁边没有 npm.cmd,此前退回 `call npm i -g`
            // 另装一份 npm 版。
            let bin = r"C:\Users\o'brien\AppData\Local\Programs\OpenAI\Codex\bin\codex.exe";
            let real = r"\\?\C:\Users\o'brien\.codex\packages\standalone\releases\0.157.0-x86_64-pc-windows-msvc\bin\codex.exe";
            let cmd = anchored_command_from_paths("codex", bin, real).unwrap();
            let expected = powershell_encoded_command(
                "$env:CODEX_NON_INTERACTIVE = '1'; $env:CODEX_INSTALL_DIR = 'C:\\Users\\o''brien\\AppData\\Local\\Programs\\OpenAI\\Codex\\bin'; $env:CODEX_HOME = 'C:\\Users\\o''brien\\.codex'; irm https://chatgpt.com/codex/install.ps1 | iex",
            );
            assert_eq!(
                cmd.split_once("-EncodedCommand ")
                    .map(|(_, encoded)| encoded),
                Some(expected.as_str())
            );
            assert!(!cmd.contains("npm"), "npm must not be used: {cmd}");
        }

        #[test]
        fn mcode_windows_npm_install_keeps_allow_scripts() {
            let (_dir, sub, bin_path) = setup_sibling("v22.19.0", "mcode.cmd", &["npm.cmd"]);
            let cmd = anchored_command_from_paths("mcode", &bin_path, &bin_path);
            let npm_full = format!("{}\\npm.cmd", sub.to_string_lossy());
            let expected = format!(
                "{} i -g @minimax-ai/code@latest{}",
                expect_quoted_path(&npm_full),
                npm_install_extra_args("mcode")
            );
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn mcode_windows_install_prefers_powershell_with_npm_fallback() {
            let install = static_fallback_command_for("mcode", ToolLifecycleAction::Install);
            let native = mcode_install_windows_command();
            assert_eq!(
                install,
                format!(
                    "{native} || call {}",
                    npm_install_command_for("mcode").unwrap()
                )
            );
            let expected_encoded = powershell_encoded_command(MCODE_INSTALL_WINDOWS_SCRIPT);
            assert_eq!(
                native
                    .split_once("-EncodedCommand ")
                    .map(|(_, encoded)| encoded),
                Some(expected_encoded.as_str())
            );
        }

        #[test]
        fn grok_windows_install_prefers_powershell_with_npm_fallback() {
            let install = static_fallback_command_for("grok", ToolLifecycleAction::Install);
            let native = grok_install_windows_command();
            assert!(
                install.starts_with(&native),
                "native installer first: {install}"
            );
            assert!(
                install.ends_with(&format!(
                    "|| call {}",
                    npm_install_command_for("grok").unwrap()
                )),
                "npm fallback should remain available: {install}"
            );
            let expected_encoded = powershell_encoded_command(GROK_INSTALL_WINDOWS_SCRIPT);
            assert_eq!(
                native
                    .split_once("-EncodedCommand ")
                    .map(|(_, encoded)| encoded),
                Some(expected_encoded.as_str())
            );
        }

        #[test]
        fn windows_no_sibling_uses_cli_update_without_package_fallback() {
            // sibling 包管理器不存在(纯独立二进制)时,仍可锚定到 CLI 自身跑官方 update。
            // 只是没有包管理器 fallback。用 claude —— codex 自 5092fe51 起一律走 npm 锚定,
            // 已不再有官方 self-update 分支,故改用仍在 prefers_official_update 的 claude 覆盖此路径。
            let (_dir, _sub, bin_path) = setup_sibling("", "claude.cmd", &[]);
            let cmd = anchored_command_from_paths("claude", &bin_path, &bin_path);
            let expected = format!("{} update", expect_quoted_path(&bin_path));
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn hermes_windows_uses_cli_update() {
            // Hermes 自带 `hermes update`,不要再回退到 py/python/pip。即便同目录有
            // npm.cmd,也不应走 npm 分支。
            let (_dir, _sub, bin_path) = setup_sibling("", "hermes.exe", &["npm.cmd"]);
            let cmd = anchored_command_from_paths("hermes", &bin_path, &bin_path);
            let expected = format!("{} update", expect_quoted_path(&bin_path));
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn hermes_windows_static_fallback_uses_powershell_installer_without_pip() {
            let install = static_fallback_command_for("hermes", ToolLifecycleAction::Install);
            assert!(
                install
                    .starts_with("powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand "),
                "should use PowerShell EncodedCommand installer: {install}"
            );
            let encoded = install
                .split_once("-EncodedCommand ")
                .map(|(_, encoded)| encoded)
                .expect("installer should include encoded command");
            assert_eq!(
                encoded,
                powershell_encoded_command(HERMES_INSTALL_WINDOWS_SCRIPT)
            );
            let install_prefix = install
                .split_once("-EncodedCommand ")
                .map(|(prefix, _)| prefix)
                .expect("installer should include encoded command");
            assert!(
                !install_prefix.contains("|")
                    && !install_prefix.contains("-Command")
                    && !install_prefix.contains("python")
                    && !install_prefix.contains("pip"),
                "should hide PowerShell pipe from cmd.exe and avoid system Python/pip: {install}"
            );

            let update = static_fallback_command("hermes");
            assert!(
                update.starts_with(
                    "hermes update || powershell -NoProfile -ExecutionPolicy Bypass -EncodedCommand "
                ),
                "should try CLI update before PowerShell installer: {update}"
            );
            let fallback = update
                .split_once("||")
                .map(|(_, fallback)| fallback)
                .expect("update should include a fallback command");
            let fallback_prefix = fallback
                .split_once("-EncodedCommand ")
                .map(|(prefix, _)| prefix)
                .expect("fallback should include encoded command");
            assert!(
                !fallback_prefix.contains('|')
                    && !fallback_prefix.contains("-Command")
                    && !update.contains("call powershell")
                    && !fallback_prefix.contains("python")
                    && !fallback_prefix.contains("pip"),
                "PowerShell fallback should be encoded, not called like a batch file or use pip: {update}"
            );
        }

        #[test]
        fn windows_path_with_space_is_double_quoted() {
            // 含空格的路径(`C:\Program Files\...`)在生成命令时必须用双引号包,否则
            // bat / cmd /C 解析会把第一个空格当 token 分隔符,后续参数串错。**精确等值断言
            // 锁定引号位置**(starts_with+contains 会放过"双引号位置错了但仍能命中"的回归)。
            let (_dir, sub, bin_path) = setup_sibling("Program Files", "codex.cmd", &["npm.cmd"]);
            let cmd = anchored_command_from_paths("codex", &bin_path, &bin_path);
            let npm_full = format!("{}\\npm.cmd", sub.to_string_lossy());
            // codex 走 npm 锚定(5092fe51),含空格的 npm 全路径仍必须被双引号包裹。
            let expected = format!(
                "{} i -g @openai/codex@latest",
                expect_quoted_path(&npm_full)
            );
            assert_eq!(cmd.as_deref(), Some(expected.as_str()));
        }

        #[test]
        fn windows_full_batch_line_for_percent_path_uses_quadruple_escape() {
            // **完整生成的 batch 行**(`call ` + anchored cmd)对含字面 `%` 的路径必须
            // 4 倍转义 `%foo%` → `%%%%foo%%%%`:.bat parser 一轮还原为 `%%foo%%`,call
            // 二轮再还原为 `%foo%` 字面。helper 单测验证的是 `win_quote_path_for_batch`
            // 内部转义,这条 integration 测验证 anchored_command_from_paths 输出 + call
            // 包装后,**最终落到 .bat 的字符串**仍然闭合两轮 expansion。
            let (_dir, sub, bin_path) = setup_sibling("path%foo%", "codex.cmd", &["npm.cmd"]);
            let anchored = anchored_command_from_paths("codex", &bin_path, &bin_path).unwrap();
            // build_tool_action_line Windows 分支最终拼的就是 `call <anchored>`(中间
            // 没有其他变换),这里直接用 format! 复刻那一步,无需暴露内部 API。
            let batch_line = format!("call {anchored}");
            // 用 `expect_quoted_path` 算 npm 全路径的期望 quoting,**同时覆盖 temp 根
            // 含空格的环境**(否则 sub 本身含空格 + 子目录 `path%foo%` 触发 4 倍 `%` 转义
            // 会让 expected 漏引号、假失败)。
            let npm_full = format!("{}\\npm.cmd", sub.to_string_lossy());
            // codex 走 npm 锚定(5092fe51)：batch 行 = `call <npm 全路径> i -g ...`，
            // 含字面 `%` 的 npm 路径仍须 4 倍转义。
            let expected = format!(
                "call {} i -g @openai/codex@latest",
                expect_quoted_path(&npm_full)
            );
            assert_eq!(batch_line, expected);
            // 双重锁定:确认 4 倍转义子串存在 + 不出现"残留的二倍转义或字面 `%foo%`"。
            assert!(
                batch_line.contains("%%%%foo%%%%"),
                "batch 行应含 4 倍转义 `%%%%foo%%%%`: {batch_line}"
            );
            assert!(
                !batch_line.contains("path%foo%"),
                "batch 行不应含未转义的字面 `%foo%`(会被 call 二次解析展开): {batch_line}"
            );
        }
    }

    /// Windows-only helpers 单测——在 macOS/Linux 上整块通过 cfg 排除,不参与 `cargo test`。
    /// Windows CI(或本机 Windows 跑 cargo test)会激活这些用例。覆盖:①双引号
    /// quoting 镜像 POSIX 版;②sibling_bin_with_ext 在 fs 上按 ext 顺序探到第一个存在的、
    /// 全部不存在/空 dir 时返 None。tempdir 提供干净 fs 沙盒。
    #[cfg(target_os = "windows")]
    mod windows_helpers {
        use super::super::*;

        #[test]
        fn win_quote_clean_path_stays_bare() {
            // 普通路径不含特殊字符 → 不加引号,命令展示干净。
            assert_eq!(
                win_quote_path_for_batch("C:\\Users\\me\\npm.cmd"),
                "C:\\Users\\me\\npm.cmd"
            );
        }

        #[test]
        fn win_quote_spaced_path_gets_quoted() {
            assert_eq!(
                win_quote_path_for_batch("C:\\Program Files\\nodejs\\npm.cmd"),
                "\"C:\\Program Files\\nodejs\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_ampersand_path_gets_quoted() {
            // `&` 是 cmd 命令分隔符,NTFS 允许在路径中出现;没有引号会让 `call C:\A&B\npm.cmd`
            // 被解析为 `call C:\A` + `B\npm.cmd` 两条命令,执行错乱。
            assert_eq!(
                win_quote_path_for_batch("C:\\Tools&Dev\\npm.cmd"),
                "\"C:\\Tools&Dev\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_parens_path_gets_quoted() {
            // `(` / `)` 在 .bat 中是代码块语义,引号内才是字面意义。
            assert_eq!(
                win_quote_path_for_batch("C:\\Foo(x86)\\npm.cmd"),
                "\"C:\\Foo(x86)\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_caret_path_gets_quoted() {
            // `^` 是 cmd 的 escape character;包引号后是字面意义。
            assert_eq!(
                win_quote_path_for_batch("C:\\foo^bar\\npm.cmd"),
                "\"C:\\foo^bar\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_percent_is_escaped_to_quadruple_percent() {
            // `%` 经历 .bat 一轮 + call 二轮 expansion,要让 call 最终看到字面 `%FOO%`
            // 需要源 .bat 里写 `%%%%FOO%%%%`(一轮 → `%%FOO%%`,二轮 → `%FOO%` 字面)。
            // 用 `%%` 二倍转义只在 echo / 直接执行场景对,call 调用时会被还原成 variable
            // reference 进而被替换。**这一条用例锁住"call 二次解析"必须被 4 倍转义闭合**。
            assert_eq!(
                win_quote_path_for_batch("C:\\path%foo%\\npm.cmd"),
                "C:\\path%%%%foo%%%%\\npm.cmd"
            );
        }

        #[test]
        fn win_quote_percent_with_space_gets_both() {
            // `%` 4 倍转义与外层引号正交——含空格触发引号、含 `%` 触发 `%%%%` 转义,叠加。
            assert_eq!(
                win_quote_path_for_batch("C:\\my %dir%\\npm.cmd"),
                "\"C:\\my %%%%dir%%%%\\npm.cmd\""
            );
        }

        #[test]
        fn win_quote_needs_quote_uses_original_path() {
            // 回归 guard:`needs_quote` 判定基于**原路径**,不能用 escape 后字符串——
            // 否则原本无 token 边界字符的路径(如 `C:\path%foo%\npm.cmd`)在 escape
            // 引入更多 `%` 后被错误识别成"需要引号"。这是实现 bug 的隐性入口。
            // 入参不含任何 token 边界字符 → 不应加外层引号、只做 `%` 4 倍转义。
            let out = win_quote_path_for_batch("C:\\foo%bar%\\npm.cmd");
            assert!(!out.starts_with('"'), "纯 `%` 路径不应加外层引号: {out}");
        }

        #[test]
        fn sibling_bin_picks_first_existing_extension() {
            // 同目录同时存在 `npm.cmd` 和 `npm.exe` 时,候选顺序 `[cmd, exe]` 应取 .cmd——
            // 这是 Node.js 官方 installer 装出来的 idiom(.cmd 是入口、.exe 是 wrapper)。
            let dir = tempfile::tempdir().unwrap();
            let cmd_path = dir.path().join("npm.cmd");
            let exe_path = dir.path().join("npm.exe");
            std::fs::write(&cmd_path, "").unwrap();
            std::fs::write(&exe_path, "").unwrap();

            let codex = dir.path().join("codex.cmd").to_string_lossy().to_string();
            let found = sibling_bin_with_ext(&codex, "npm", &["cmd", "exe"]).unwrap();
            assert_eq!(found, cmd_path.to_string_lossy());
        }

        #[test]
        fn sibling_bin_volta_prefers_exe() {
            // Volta 是 Rust 写的 native binary,扩展名顺序应是 [exe, cmd]——若只有 .exe
            // 存在(常见情形),探到的就是它。
            let dir = tempfile::tempdir().unwrap();
            let exe_path = dir.path().join("volta.exe");
            std::fs::write(&exe_path, "").unwrap();

            let codex = dir.path().join("codex.exe").to_string_lossy().to_string();
            let found = sibling_bin_with_ext(&codex, "volta", &["exe", "cmd"]).unwrap();
            assert_eq!(found, exe_path.to_string_lossy());
        }

        #[test]
        fn sibling_bin_returns_none_when_none_exist() {
            // 同目录下没有任何候选 → None,锚定层据此退到静态命令。
            let dir = tempfile::tempdir().unwrap();
            let codex = dir.path().join("codex.cmd").to_string_lossy().to_string();
            assert!(sibling_bin_with_ext(&codex, "npm", &["cmd", "exe"]).is_none());
        }

        #[test]
        fn sibling_bin_returns_none_when_no_parent() {
            // bin_path 没有目录部分(纯文件名) → parent_dir 空串 → 返 None。
            assert!(sibling_bin_with_ext("codex.cmd", "npm", &["cmd"]).is_none());
        }

        #[test]
        fn wsl_hermes_command_uses_unix_installer_not_powershell_or_pip() {
            // 跨 wsl.exe 边界后跑的是 Linux,Windows PowerShell installer 不适用;
            // 也不要再走 python3/python pip 链,避免 Python 版本/pyenv shim 问题。
            let update_cmd =
                wsl_tool_action_shell_command("hermes", ToolLifecycleAction::Update).unwrap();
            assert!(
                update_cmd.starts_with("hermes update || bash -c 'tmp=$(mktemp) && curl -fsSL "),
                "WSL hermes 更新应先尝试 CLI 自更新再回退官方 installer,得到: {update_cmd}"
            );
            let fallback = update_cmd
                .split_once("||")
                .map(|(_, fallback)| fallback)
                .expect("update should include installer fallback");
            assert!(
                !fallback.contains('|')
                    && fallback.contains(" -o $tmp && bash $tmp")
                    && !update_cmd.contains("powershell")
                    && !update_cmd.contains("pip"),
                "WSL hermes fallback 不能依赖 pipefail/Windows installer/pip,得到: {update_cmd}"
            );

            let install_cmd =
                wsl_tool_action_shell_command("hermes", ToolLifecycleAction::Install).unwrap();
            assert!(
                install_cmd.starts_with("bash -c 'tmp=$(mktemp) && curl -fsSL "),
                "WSL hermes 安装应直接走官方 Unix installer,得到: {install_cmd}"
            );
            assert!(
                !install_cmd.contains('|') && install_cmd.contains(" -o $tmp && bash $tmp"),
                "WSL hermes 安装不应依赖 pipefail,得到: {install_cmd}"
            );
        }

        #[test]
        fn wsl_hermes_dashboard_runs_inside_the_distro_login_shell() {
            let line = wsl_hermes_dashboard_line("Ubuntu-22.04").expect("valid WSL command line");
            assert_eq!(
                line,
                r#"wsl.exe -d Ubuntu-22.04 -- sh -c "\"${SHELL:-sh}\" -lic 'hermes dashboard'""#
            );
            assert!(wsl_hermes_dashboard_line("Ubuntu & calc").is_err());
        }

        #[test]
        fn wsl_hermes_install_line_does_not_depend_on_outer_pipefail() {
            let line = build_wsl_tool_action_line("Ubuntu", HERMES_INSTALL_UNIX, None, None)
                .expect("valid WSL command line");
            assert!(line.starts_with("wsl.exe -d Ubuntu -- sh -c "));
            assert!(
                !line.contains("| bash") && line.contains(" -o $tmp && bash $tmp"),
                "WSL 子 shell 内不能出现 curl 管道安装器: {line}"
            );
        }

        #[test]
        fn wsl_install_uses_posix_install_priority() {
            let claude =
                wsl_tool_action_shell_command("claude", ToolLifecycleAction::Install).unwrap();
            assert!(
                claude.starts_with("bash -c 'tmp=$(mktemp) && curl -fsSL https://claude.ai/install.sh ")
                    && claude.ends_with(" || npm i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code"),
                "WSL claude install should prefer native POSIX installer with npm fallback: {claude}"
            );
            assert!(!claude.contains("| bash"));

            let opencode =
                wsl_tool_action_shell_command("opencode", ToolLifecycleAction::Install).unwrap();
            assert!(
                opencode.starts_with(
                    "bash -c 'tmp=$(mktemp) && curl -fsSL https://opencode.ai/install "
                ) && opencode.contains(" || npm i -g opencode-ai@latest"),
                "WSL opencode install should prefer native POSIX installer with npm fallback: {opencode}"
            );
            assert!(!opencode.contains("| bash"));

            let codex =
                wsl_tool_action_shell_command("codex", ToolLifecycleAction::Install).unwrap();
            assert_eq!(codex, "npm i -g @openai/codex@latest");

            let grok = wsl_tool_action_shell_command("grok", ToolLifecycleAction::Install).unwrap();
            assert!(
                grok.starts_with(
                    "bash -c 'tmp=$(mktemp) && curl -fsSL https://x.ai/cli/install.sh "
                ) && grok.ends_with(&format!(" || {}", npm_install_command_for("grok").unwrap())),
                "WSL grok install should prefer native POSIX installer with npm fallback: {grok}"
            );
            assert!(!grok.contains("| bash"));
        }

        #[test]
        fn wsl_npm_tools_use_posix_update_chain_without_batch_call() {
            // WSL 内跑的是 POSIX shell,不能带 Windows batch 的 `call`。同时 update
            // fallback 仍应先尝试官方 CLI 自升级。
            let cmd = wsl_tool_action_shell_command("claude", ToolLifecycleAction::Update).unwrap();
            assert_eq!(
                cmd,
                "claude update || npm i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code"
            );
        }
    }

    /// `infer_install_source` 是判定锚定 idiom 的入口——nvm/homebrew/volta/pnpm/...
    /// 各对应不同的升级命令形态。函数内部已 `replace('\\','/').to_ascii_lowercase()`
    /// 归一化,Windows 反斜杠 + 大小写差异在此处不需要分平台。这里固化"哪条路径
    /// 算哪种来源"的归类断言,避免未来调整子串顺序时静默改变分类。
    mod install_source_classification {
        use super::super::*;
        use std::path::Path;

        #[test]
        fn macos_homebrew_caskroom_is_homebrew() {
            assert_eq!(
                infer_install_source(Path::new("/opt/homebrew/Caskroom/codex/0.146.0/bin/codex")),
                "homebrew"
            );
            // Intel prefix 不含 `/homebrew/`，必须靠 `/caskroom/` 本身命中。
            assert_eq!(
                infer_install_source(Path::new("/usr/local/Caskroom/codex/0.146.0/bin/codex")),
                "homebrew"
            );
        }

        #[test]
        fn intel_cask_launcher_uses_resolved_target() {
            // 标准 Intel cask：PATH 入口是 `/usr/local/bin/codex`，真身才在 Caskroom。
            // 只看 launcher 会落到 `system`，徽章和冲突诊断都会错。
            assert_eq!(
                infer_install_source(Path::new("/usr/local/bin/codex")),
                "system"
            );
            assert_eq!(
                infer_install_source_for_install(
                    Path::new("/usr/local/bin/codex"),
                    Path::new("/usr/local/Caskroom/codex/0.146.0/bin/codex"),
                ),
                "homebrew"
            );
            // nvm shim 仍按 launcher 分类，不能被真身路径抢走。
            assert_eq!(
                infer_install_source_for_install(
                    Path::new("/Users/me/.nvm/versions/node/v22.0.0/bin/codex"),
                    Path::new(
                        "/Users/me/.nvm/versions/node/v22.0.0/lib/node_modules/@openai/codex/bin/codex.js"
                    ),
                ),
                "nvm"
            );
        }

        #[test]
        fn macos_volta_with_dot_prefix() {
            assert_eq!(
                infer_install_source(Path::new("/Users/me/.volta/bin/codex")),
                "volta"
            );
        }

        #[test]
        fn windows_volta_localappdata_no_dot() {
            // `%LOCALAPPDATA%\Volta\bin\codex.exe` —— 没有前导点,靠兜底的 `/volta/`
            // 命中(归一化后小写)。如果只识别 `/.volta/`,Windows 这一类会落到 system。
            assert_eq!(
                infer_install_source(Path::new(
                    "C:\\Users\\me\\AppData\\Local\\Volta\\bin\\codex.exe"
                )),
                "volta"
            );
        }

        #[test]
        fn windows_pnpm_localappdata() {
            // `%LOCALAPPDATA%\pnpm\codex.cmd` —— pnpm 全局 bin 目录,识别为 pnpm 后
            // 锚定命令走 `pnpm add -g <pkg>@latest`,而不是 sibling npm。
            assert_eq!(
                infer_install_source(Path::new("C:\\Users\\me\\AppData\\Local\\pnpm\\codex.cmd")),
                "pnpm"
            );
        }

        #[test]
        fn windows_nvm_falls_back_to_system() {
            // nvm-windows 安装的工具路径不含 `.nvm`(它通常装在 `%APPDATA%\nvm` 或
            // `C:\Program Files\nodejs` symlink),刻意不识别成专属 source——锚定层
            // 会按 system → sibling npm.cmd 处理,跟 nvm-windows 的实际 idiom 一致
            // (它的全局包就是当前选中的 node 的 npm 装的)。
            assert_eq!(
                infer_install_source(Path::new(
                    "C:\\Users\\me\\AppData\\Roaming\\nvm\\v22.0.0\\codex.cmd"
                )),
                "system"
            );
        }

        #[test]
        fn windows_scoop_still_identified() {
            // Scoop 已有 `/scoop/` 分支;我们的 6 个工具都不是 scoop formula,所以这条
            // 实际不影响锚定决策(锚定层会用 sibling npm.cmd),但归类保留方便未来。
            assert_eq!(
                infer_install_source(Path::new("C:\\Users\\me\\scoop\\shims\\codex.cmd")),
                "scoop"
            );
        }
    }

    /// 锚定升级命令生成：用真实勘察到的安装路径固化为回归断言——
    /// 一台机器上 4 个工具恰好对应 4 种升级方式（原生 self-update / brew / nvm npm /
    /// homebrew npm），任何改动若打破其中一种都会立刻被这些用例拦下。
    #[cfg(not(target_os = "windows"))]
    mod anchored_upgrade {
        use super::super::*;
        use std::path::Path;

        fn inst(path: &str, is_default: bool) -> ToolInstallation {
            ToolInstallation {
                path: path.to_string(),
                version: None,
                runnable: true,
                error: None,
                exited_with_error: false,
                source: infer_install_source(Path::new(path)).to_string(),
                is_path_default: is_default,
                // 测试场景下不需要走 fs canonicalize——POSIX 锚定测试关心的是
                // path/real 都被传给 anchored_command_from_paths 的纯字符串判定,
                // 已有用例(brew_formula_extraction / claude_native_*)是直接
                // 调 anchored_command_from_paths,不通过 installs_anchored_command,
                // 这里 real 是给上层 default_install + read 用,填同值即可。
                real: std::path::PathBuf::from(path),
            }
        }

        #[test]
        fn claude_native_installer_uses_self_update() {
            // ~/.local/bin/claude → 真身在 ~/.local/share/claude/versions/,自带 self-update;
            // 它不归 npm 管,且在 PATH 里比 nvm/homebrew 更靠前,用 npm 升级纯属白装。
            // **绝对路径调用 launcher** 避免 GUI 非登录 `bash -c` 时 PATH 没有
            // ~/.local/bin 导致 `claude: not found`(exit 127)而失败。
            let cmd = anchored_command_from_paths(
                "claude",
                "/Users/me/.local/bin/claude",
                "/Users/me/.local/share/claude/versions/2.1.146",
            );
            assert_eq!(cmd.as_deref(), Some("/Users/me/.local/bin/claude update"));
        }

        #[test]
        fn grok_native_installer_uses_self_update_with_installer_fallback() {
            // ~/.grok/bin/grok is a launcher symlink into ~/.grok/downloads.
            // Updating it through npm would create or mutate a different install.
            let cmd = anchored_command_from_paths(
                "grok",
                "/Users/me/.grok/bin/grok",
                "/Users/me/.grok/downloads/grok-macos-aarch64",
            );
            assert_eq!(
                cmd.as_deref(),
                Some(
                    format!(
                        "npm_config_allow_scripts=@xai-official/grok npm_config_ignore_scripts=false npm_config_include=optional /Users/me/.grok/bin/grok update || {GROK_INSTALL_UNIX}"
                    )
                    .as_str()
                )
            );
        }

        #[test]
        fn grok_native_update_falls_back_to_installer_not_npm() {
            // 反向锁定:`grok update` 内部靠 `npm view` + `npm i -g` 完成升级,GUI 的窄
            // PATH 下会 ENOENT。fallback 必须是官方 installer —— 换成 `npm i -g` 就与
            // primary 同源(无 node / 镜像缺 tarball 时一起失败),`||` 形同虚设。
            let cmd = anchored_command_from_paths(
                "grok",
                "/Users/me/.grok/bin/grok",
                "/Users/me/.grok/downloads/grok-macos-aarch64",
            )
            .expect("native grok should anchor");
            let (_, fallback) = cmd.split_once(" || ").unwrap();
            assert!(fallback.contains("x.ai/cli/install.sh"), "{cmd}");
            assert!(
                !fallback.contains("npm"),
                "npm must not be the fallback: {cmd}"
            );
        }

        #[test]
        fn grok_custom_bin_dir_is_native_when_target_is_official_download() {
            let cmd = anchored_command_from_paths(
                "grok",
                "/Users/me/bin/grok",
                "/Users/me/.grok/downloads/grok-macos-aarch64",
            );
            assert_eq!(
                cmd.as_deref(),
                Some(
                    format!(
                        "npm_config_allow_scripts=@xai-official/grok npm_config_ignore_scripts=false npm_config_include=optional /Users/me/bin/grok update || {GROK_INSTALL_UNIX}"
                    )
                    .as_str()
                )
            );
        }

        #[test]
        fn gemini_homebrew_formula_uses_brew_upgrade() {
            // /opt/homebrew/bin/gemini → Cellar/gemini-cli/...:是 brew formula 而非 npm 全局包,
            // 且 formula 名(gemini-cli) ≠ npm 包名(@google/gemini-cli)。
            // **brew 与 formula 入口同目录**,用 `<dir>/brew` 绝对路径调用,避免 GUI
            // 非登录 `bash -c` 时 PATH 没有 /opt/homebrew/bin 导致 `brew: not found`。
            let cmd = anchored_command_from_paths(
                "gemini",
                "/opt/homebrew/bin/gemini",
                "/opt/homebrew/Cellar/gemini-cli/0.13.0/libexec/lib/node_modules/@google/gemini-cli/dist/index.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/opt/homebrew/bin/brew upgrade gemini-cli")
            );
        }

        #[test]
        fn codex_homebrew_formula_uses_brew_not_self_update() {
            // Homebrew formula 归 brew 管理;即使 Codex 有 self-update,也不先改动
            // Cellar 内的安装内容。
            let cmd = anchored_command_from_paths(
                "codex",
                "/opt/homebrew/bin/codex",
                "/opt/homebrew/Cellar/codex/1.2.3/bin/codex",
            );
            assert_eq!(cmd.as_deref(), Some("/opt/homebrew/bin/brew upgrade codex"));
        }

        #[test]
        fn codex_homebrew_cask_uses_brew_upgrade_cask() {
            // `/opt/homebrew/bin/codex` → Caskroom/codex/...:是 brew cask 而非 npm 全局包。
            // 若误走 sibling `npm i -g @openai/codex`，npm bin-links 会因
            // `/opt/homebrew/bin/codex` 不属于它而 EEXIST（#6562）。
            let cmd = anchored_command_from_paths(
                "codex",
                "/opt/homebrew/bin/codex",
                "/opt/homebrew/Caskroom/codex/0.146.0/bin/codex",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/opt/homebrew/bin/brew upgrade --cask codex")
            );
        }

        #[test]
        fn intel_homebrew_cask_without_homebrew_prefix_still_uses_brew() {
            // Intel 默认 prefix 是 `/usr/local`，Caskroom 路径不含 `/homebrew/`。
            // 真身解析必须靠 `Caskroom` 段本身，不能依赖 prefix 子串。
            let cmd = anchored_command_from_paths(
                "codex",
                "/usr/local/bin/codex",
                "/usr/local/Caskroom/codex/0.146.0/bin/codex",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/usr/local/bin/brew upgrade --cask codex")
            );
        }

        #[test]
        fn gemini_nvm_anchors_to_npm_without_cli_update() {
            let cmd = anchored_command_from_paths(
                "gemini",
                "/Users/me/.nvm/versions/node/v22.14.0/bin/gemini",
                "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@google/gemini-cli/dist/index.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some(
                    "PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm i -g @google/gemini-cli@latest"
                )
            );
        }

        #[test]
        fn grok_nvm_anchors_to_npm_without_cli_update() {
            let cmd = anchored_command_from_paths(
                "grok",
                "/Users/me/.nvm/versions/node/v22.14.0/bin/grok",
                "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@xai-official/grok/bin/grok",
            );
            assert_eq!(
                cmd.as_deref(),
                Some(
                    "PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm i -g @xai-official/grok@latest --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok"
                )
            );
        }

        #[test]
        fn mcode_script_install_reruns_official_installer_in_place() {
            // `~/.minimax-code/bin/mcode` 是官方 installer 的 sh launcher，不归全局 npm 管；
            // `mcode update` 非 TTY 下不安装，所以改为带 MCODE_INSTALL_DIR 重跑 installer。
            let expected =
                format!("MCODE_INSTALL_DIR='/Users/me/.minimax-code' {MCODE_INSTALL_UNIX}");
            for bin_path in [
                "/Users/me/.minimax-code/bin/mcode",
                "/Users/me/.local/bin/mcode",
            ] {
                let cmd = anchored_command_from_paths(
                    "mcode",
                    bin_path,
                    "/Users/me/.minimax-code/bin/mcode",
                );
                assert_eq!(cmd.as_deref(), Some(expected.as_str()));
            }
        }

        #[test]
        fn mcode_nvm_anchors_to_npm_with_allow_scripts() {
            let cmd = anchored_command_from_paths(
                "mcode",
                "/Users/me/.nvm/versions/node/v22.19.0/bin/mcode",
                "/Users/me/.nvm/versions/node/v22.19.0/lib/node_modules/@minimax-ai/code/cli.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some(
                    "PATH='/Users/me/.nvm/versions/node/v22.19.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.19.0/bin/npm i -g @minimax-ai/code@latest --ignore-scripts=false --include=optional \"--allow-scripts=@minimax-ai/code,better-sqlite3\""
                )
            );
        }

        #[test]
        fn claude_npm_sources_scope_postinstall_settings_to_primary_and_full_fallback() {
            for (path, dir) in [
                (
                    "/Users/me/.nvm/versions/node/v22/bin/claude",
                    "/Users/me/.nvm/versions/node/v22/bin",
                ),
                (
                    "/Users/me/.local/share/fnm_multishells/12345_abc/bin/claude",
                    "/Users/me/.local/share/fnm_multishells/12345_abc/bin",
                ),
                (
                    "/Users/me/.local/share/mise/installs/node/22/bin/claude",
                    "/Users/me/.local/share/mise/installs/node/22/bin",
                ),
                ("/opt/homebrew/bin/claude", "/opt/homebrew/bin"),
            ] {
                let mut install = inst(path, true);
                install.real = PathBuf::from(format!(
                    "{}/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe",
                    dir.trim_end_matches("/bin")
                ));
                let command = installs_anchored_command("claude", &[install]).unwrap();
                let (primary, fallback) = command.split_once(" || ").unwrap();
                assert_eq!(
                    primary,
                    format!(
                        "npm_config_allow_scripts=@anthropic-ai/claude-code npm_config_ignore_scripts=false npm_config_include=optional {path} update"
                    )
                );
                assert_eq!(
                    fallback,
                    format!(
                        "PATH='{dir}':\"$PATH\" {dir}/npm i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code"
                    )
                );
            }
        }

        #[test]
        fn opencode_npm_sources_scope_postinstall_settings_to_primary_and_fallback() {
            // opencode-ai's postinstall lays the platform binary over the text
            // placeholder at bin/opencode.exe; npm 12 blocks it by default (#8863).
            let dir = "/Users/me/.nvm/versions/node/v22/bin";
            let path = format!("{dir}/opencode");
            let mut install = inst(&path, true);
            install.real = PathBuf::from(
                "/Users/me/.nvm/versions/node/v22/lib/node_modules/opencode-ai/bin/opencode.exe",
            );
            let command = installs_anchored_command("opencode", &[install]).unwrap();
            let (primary, fallback) = command.split_once(" || ").unwrap();
            assert_eq!(
                primary,
                format!(
                    "npm_config_allow_scripts=opencode-ai npm_config_ignore_scripts=false npm_config_include=optional {path} upgrade"
                )
            );
            assert_eq!(
                fallback,
                format!(
                    "PATH='{dir}':\"$PATH\" {dir}/npm i -g opencode-ai@latest --ignore-scripts=false --include=optional --allow-scripts=opencode-ai"
                )
            );
            assert_eq!(
                anchored_command_from_paths(
                    "opencode",
                    "/usr/local/bin/opencode",
                    "/usr/local/lib/node_modules/opencode-ai/bin/opencode.exe",
                ),
                Some(
                    "npm_config_allow_scripts=opencode-ai npm_config_ignore_scripts=false npm_config_include=optional /usr/local/bin/opencode upgrade"
                        .to_string()
                )
            );
        }

        #[test]
        fn claude_system_npm_prefixes_scope_postinstall_settings_without_fallback() {
            // Launchers classified as `system` have no sibling-npm fallback, so the
            // primary update is their only chance to keep npm 12 from breaking Claude.
            for (path, real) in [
                (
                    "/usr/local/bin/claude",
                    "/usr/local/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe",
                ),
                (
                    "/Users/me/.npm-global/bin/claude",
                    "/Users/me/.npm-global/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe",
                ),
                (
                    "/usr/bin/claude",
                    "/usr/lib/node_modules/@anthropic-ai/claude-code/cli.js",
                ),
            ] {
                assert_eq!(
                    anchored_command_from_paths("claude", path, real),
                    Some(format!(
                        "npm_config_allow_scripts=@anthropic-ai/claude-code npm_config_ignore_scripts=false npm_config_include=optional {path} update"
                    ))
                );
            }
        }

        #[test]
        fn claude_other_install_sources_keep_their_update_policy() {
            for (path, real, expected) in [
                (
                    "/Users/me/.nvm/versions/node/v22/bin/claude",
                    "/Users/me/.local/share/claude/versions/2.1.146",
                    "/Users/me/.nvm/versions/node/v22/bin/claude update",
                ),
                (
                    "/opt/homebrew/bin/claude",
                    "/opt/homebrew/Cellar/claude-code/2.1.146/bin/claude",
                    "/opt/homebrew/bin/brew upgrade claude-code",
                ),
                (
                    "/Users/me/.volta/bin/claude",
                    "/Users/me/.volta/bin/claude",
                    "/Users/me/.volta/bin/claude update || /Users/me/.volta/bin/volta install @anthropic-ai/claude-code",
                ),
                (
                    "/Users/me/.bun/bin/claude",
                    "/Users/me/.bun/bin/claude",
                    "/Users/me/.bun/bin/claude update || /Users/me/.bun/bin/bun add -g @anthropic-ai/claude-code@latest",
                ),
                (
                    "/usr/local/bin/claude",
                    "/usr/local/bin/claude",
                    "/usr/local/bin/claude update",
                ),
            ] {
                assert_eq!(
                    anchored_command_from_paths("claude", path, real).as_deref(),
                    Some(expected)
                );
            }
        }

        #[test]
        fn claude_updater_child_inherits_scoped_settings_without_parent_leakage() {
            use std::os::unix::fs::PermissionsExt;
            use std::process::Command;

            let temp = tempfile::tempdir().expect("temp dir should be created");
            let bin = temp.path().join("home dir/.nvm/versions/node/v22/bin");
            std::fs::create_dir_all(&bin).unwrap();
            let claude = bin.join("claude");
            let npm = bin.join("npm");
            std::fs::write(
                &claude,
                "#!/bin/sh\n[ \"$1\" = update ] || exit 99\nnpm install -g @anthropic-ai/claude-code@latest\nexit \"$TEST_CLAUDE_EXIT\"\n",
            )
            .unwrap();
            std::fs::write(
                &npm,
                "#!/bin/sh\nprintf '%s|%s|%s|%s\n' \"\u{24}{npm_config_allow_scripts-unset}\" \"\u{24}{npm_config_ignore_scripts-unset}\" \"\u{24}{npm_config_include-unset}\" \"\u{24}*\"\n[ \"$1\" != i ] || exit \"$TEST_FALLBACK_EXIT\"\n",
            )
            .unwrap();
            for executable in [&claude, &npm] {
                std::fs::set_permissions(executable, std::fs::Permissions::from_mode(0o755))
                    .unwrap();
            }

            let mut install = inst(&claude.to_string_lossy(), true);
            install.real = temp.path().join(
                "home dir/.nvm/versions/node/v22/lib/node_modules/@anthropic-ai/claude-code/bin/claude.exe",
            );
            let command = installs_anchored_command("claude", &[install]).unwrap();
            let script = format!(
                "set -e\n{command}\nprintf 'after|%s|%s|%s\n' \"$npm_config_allow_scripts\" \"$npm_config_ignore_scripts\" \"$npm_config_include\""
            );
            let primary = "@anthropic-ai/claude-code|false|optional|install -g @anthropic-ai/claude-code@latest\n";
            let fallback = "previous-package|true|prod|i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code\n";
            for (primary_exit, fallback_exit, expected_code, expected_output) in [
                (
                    "0",
                    "0",
                    0,
                    format!("{primary}after|previous-package|true|prod\n"),
                ),
                (
                    "1",
                    "0",
                    0,
                    format!("{primary}{fallback}after|previous-package|true|prod\n"),
                ),
                ("1", "42", 42, format!("{primary}{fallback}")),
            ] {
                let output = Command::new("/bin/bash")
                    .args(["-c", &script])
                    .env("PATH", format!("{}:/usr/bin:/bin", bin.display()))
                    .env("npm_config_allow_scripts", "previous-package")
                    .env("npm_config_ignore_scripts", "true")
                    .env("npm_config_include", "prod")
                    .env("TEST_CLAUDE_EXIT", primary_exit)
                    .env("TEST_FALLBACK_EXIT", fallback_exit)
                    .output()
                    .expect("Claude update chain should start");
                assert_eq!(
                    output.status.code(),
                    Some(expected_code),
                    "{}",
                    String::from_utf8_lossy(&output.stderr)
                );
                assert_eq!(String::from_utf8_lossy(&output.stdout), expected_output);
            }
        }

        #[test]
        fn codex_nvm_anchors_to_that_npm() {
            // Codex 不走 self-update（`codex update` 在 npm 安装上只是裸 `npm install -g`，
            // 却会假成功掩盖平台二进制漏装）——直接锚定到同一个 node 的 npm，而非 PATH
            // 第一个 npm。损坏时的 uninstall+install 自愈见 codex_missing_platform_binary_*。
            let cmd = anchored_command_from_paths(
                "codex",
                "/Users/me/.nvm/versions/node/v22.14.0/bin/codex",
                "/Users/me/.nvm/versions/node/v22.14.0/lib/node_modules/@openai/codex/bin/codex.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm i -g @openai/codex@latest")
            );
        }

        #[test]
        fn homebrew_npm_global_package_anchors_not_brew() {
            // openclaw 装在 Homebrew node 的全局目录(lib/node_modules，非 Cellar)：
            // 是 npm 全局包，官方 update 失败后走 npm 锚定而非 brew upgrade。
            let cmd = anchored_command_from_paths(
                "openclaw",
                "/opt/homebrew/bin/openclaw",
                "/opt/homebrew/lib/node_modules/openclaw/openclaw.mjs",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/opt/homebrew/bin/openclaw update --yes || PATH='/opt/homebrew/bin':\"$PATH\" /opt/homebrew/bin/npm i -g openclaw@latest")
            );
        }

        #[test]
        fn volta_self_update_chain_anchors_to_volta() {
            // `~/.volta/bin` 通常不在 GUI 非登录 `bash -c` 的 PATH 里,且用户可能
            // PATH 上还有另一份 volta → 必须绝对路径锚定到命令行命中的这一份。
            // 用 openclaw（仍在 prefers_official_update）覆盖 volta 分支的 self-update 链;
            // codex 已改为不 self-update（见 codex_volta_anchors_to_volta_install）。
            let cmd = anchored_command_from_paths(
                "openclaw",
                "/Users/me/.volta/bin/openclaw",
                "/Users/me/.volta/tools/image/packages/openclaw/lib/node_modules/openclaw",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/Users/me/.volta/bin/openclaw update --yes || /Users/me/.volta/bin/volta install openclaw")
            );
        }

        #[test]
        fn codex_volta_anchors_to_volta_install() {
            // codex 锚定到命令行命中的那份 volta，但不 self-update：纯 `volta install`。
            let cmd = anchored_command_from_paths(
                "codex",
                "/Users/me/.volta/bin/codex",
                "/Users/me/.volta/tools/image/packages/codex/lib/node_modules/@openai/codex",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/Users/me/.volta/bin/volta install @openai/codex")
            );
        }

        #[test]
        fn bun_uses_bun_add() {
            // OpenCode 先跑官方 upgrade;失败后 bun 同 volta:绝对路径写回原安装源。
            let cmd = anchored_command_from_paths(
                "opencode",
                "/Users/me/.bun/bin/opencode",
                "/Users/me/.bun/install/global/node_modules/opencode-ai/bin/opencode",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/Users/me/.bun/bin/opencode upgrade || /Users/me/.bun/bin/bun add -g opencode-ai@latest")
            );
        }

        #[test]
        fn omp_updates_through_its_own_updater_with_source_specific_fallbacks() {
            // Homebrew owns its formula even though `omp update` would also delegate.
            assert_eq!(
                anchored_command_from_paths(
                    "omp",
                    "/opt/homebrew/bin/omp",
                    "/opt/homebrew/Cellar/omp/14.1.0/bin/omp",
                )
                .as_deref(),
                Some("/opt/homebrew/bin/brew upgrade omp")
            );
            // A bun global keeps bun as the fallback.
            assert_eq!(
                anchored_command_from_paths(
                    "omp",
                    "/Users/me/.bun/bin/omp",
                    "/Users/me/.bun/install/global/node_modules/@oh-my-pi/pi-coding-agent/src/cli.ts",
                )
                .as_deref(),
                Some("/Users/me/.bun/bin/omp update || /Users/me/.bun/bin/bun add -g @oh-my-pi/pi-coding-agent@latest")
            );
            // The installer's prebuilt binary updates itself; no npm fallback is invented.
            assert_eq!(
                anchored_command_from_paths(
                    "omp",
                    "/Users/me/.local/bin/omp",
                    "/Users/me/.local/bin/omp",
                )
                .as_deref(),
                Some("/Users/me/.local/bin/omp update")
            );
            // npm and other node managers cannot run omp, so they never become a fallback.
            assert_eq!(
                anchored_command_from_paths(
                    "omp",
                    "/Users/me/.nvm/versions/node/v22.0.0/bin/omp",
                    "/Users/me/.nvm/versions/node/v22.0.0/lib/node_modules/@oh-my-pi/pi-coding-agent/src/cli.ts",
                )
                .as_deref(),
                Some("/Users/me/.nvm/versions/node/v22.0.0/bin/omp update")
            );
            // `omp update` exits 0 on Nix without updating: leave it to the user.
            assert_eq!(
                anchored_command_from_paths(
                    "omp",
                    "/Users/me/.nix-profile/bin/omp",
                    "/nix/store/abc-omp-14.1.0/bin/omp",
                ),
                None
            );
        }

        #[test]
        fn volta_path_with_space_is_quoted() {
            // volta 分支用 `<dir>/volta`,目录含空格时同样要 POSIX 引号包裹。
            let cmd = anchored_command_from_paths(
                "codex",
                "/Users/my name/.volta/bin/codex",
                "/Users/my name/.volta/tools/image/packages/codex/lib/node_modules/@openai/codex",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("'/Users/my name/.volta/bin/volta' install @openai/codex")
            );
        }

        #[test]
        fn bun_path_with_space_is_quoted() {
            // bun 分支与 volta 共享 sibling_bin + quote_path_if_spaced,
            // 这条用例锁住 `bun add -g` 命令头部的引号包裹形态。
            let cmd = anchored_command_from_paths(
                "opencode",
                "/Users/my name/.bun/bin/opencode",
                "/Users/my name/.bun/install/global/node_modules/opencode-ai/bin/opencode",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("'/Users/my name/.bun/bin/opencode' upgrade || '/Users/my name/.bun/bin/bun' add -g opencode-ai@latest")
            );
        }

        #[test]
        fn hermes_uses_cli_update_anchor() {
            // Hermes 自带 `hermes update`;锚定到命令行默认那处 CLI,避免 cc-switch 猜
            // 系统 Python/pip 时撞上 Python >=3.11 或 pyenv shim 问题。
            let cmd = anchored_command_from_paths(
                "hermes",
                "/usr/local/bin/hermes",
                "/usr/local/bin/hermes",
            );
            assert_eq!(cmd.as_deref(), Some("/usr/local/bin/hermes update"));
        }

        #[test]
        fn agy_updates_itself_even_from_a_homebrew_cask() {
            // The cask is auto_updates, so `brew upgrade` skips it; agy replaces its own binary.
            for real in [
                "/opt/homebrew/Caskroom/antigravity-cli/1.3.4,5524158944247808/antigravity",
                "/opt/homebrew/bin/agy",
            ] {
                let cmd = anchored_command_from_paths("agy", "/opt/homebrew/bin/agy", real);
                assert_eq!(cmd.as_deref(), Some("/opt/homebrew/bin/agy update"));
            }
        }

        #[test]
        fn opencode_native_install_uses_cli_upgrade_without_package_fallback() {
            // opencode install.sh 装到 ~/.opencode/bin（独立二进制、无同级 npm）：
            // 不能锚定到 `<dir>/npm`（必失败），但可以锚定到 CLI 自身跑官方 upgrade。
            let cmd = anchored_command_from_paths(
                "opencode",
                "/Users/me/.opencode/bin/opencode",
                "/Users/me/.opencode/bin/opencode",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("/Users/me/.opencode/bin/opencode upgrade")
            );
        }

        #[test]
        fn go_bin_opencode_uses_cli_upgrade_without_package_fallback() {
            // ~/go/bin 同理：无同级 npm，但 OpenCode 官方 upgrade 可由 CLI 自己处理。
            let cmd = anchored_command_from_paths(
                "opencode",
                "/Users/me/go/bin/opencode",
                "/Users/me/go/bin/opencode",
            );
            assert_eq!(cmd.as_deref(), Some("/Users/me/go/bin/opencode upgrade"));
        }

        #[test]
        fn fnm_install_anchors_to_that_npm() {
            // fnm 是自带同级 npm 的 node 管理器 → 锚定到那处的 npm。
            let cmd = anchored_command_from_paths(
                "codex",
                "/Users/me/.local/share/fnm_multishells/12345_abc/bin/codex",
                "/Users/me/.local/share/fnm_multishells/12345_abc/lib/node_modules/@openai/codex/bin/codex.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some(
                    "PATH='/Users/me/.local/share/fnm_multishells/12345_abc/bin':\"$PATH\" /Users/me/.local/share/fnm_multishells/12345_abc/bin/npm i -g @openai/codex@latest"
                )
            );
        }

        #[test]
        fn path_with_space_is_quoted() {
            let cmd = anchored_command_from_paths(
                "codex",
                "/Users/my name/.nvm/versions/node/v22/bin/codex",
                "/Users/my name/.nvm/versions/node/v22/lib/node_modules/@openai/codex/bin/codex.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("PATH='/Users/my name/.nvm/versions/node/v22/bin':\"$PATH\" '/Users/my name/.nvm/versions/node/v22/bin/npm' i -g @openai/codex@latest")
            );
        }

        #[test]
        fn npm_anchor_supplies_sibling_node_to_env_shebang() {
            use std::os::unix::fs::PermissionsExt;
            use std::process::Command;

            let temp = tempfile::tempdir().expect("temp dir should be created");
            let bin = temp.path().join("home dir/.nvm/versions/node/v22.14.0/bin");
            std::fs::create_dir_all(&bin).expect("node bin should be created");

            let marker = temp.path().join("sibling-node-used");
            let node = bin.join("node");
            let npm = bin.join("npm");
            std::fs::write(
                &node,
                format!(
                    "#!/bin/sh\nprintf sibling-node > {}\n",
                    shell_single_quote(&marker.to_string_lossy())
                ),
            )
            .expect("fake node should be written");
            std::fs::write(&npm, "#!/usr/bin/env node\n").expect("fake npm should be written");
            for executable in [&node, &npm] {
                let mut permissions = std::fs::metadata(executable)
                    .expect("fake executable metadata should exist")
                    .permissions();
                permissions.set_mode(0o755);
                std::fs::set_permissions(executable, permissions)
                    .expect("fake executable should be executable");
            }

            let codex = bin.join("codex").to_string_lossy().into_owned();
            let real = temp
                .path()
                .join("home dir/.nvm/versions/node/v22.14.0/lib/node_modules/@openai/codex/bin/codex.js")
                .to_string_lossy()
                .into_owned();
            let command = anchored_command_from_paths("codex", &codex, &real)
                .expect("nvm codex should produce an anchored npm command");
            let output = Command::new("/bin/bash")
                .args(["-c", &command])
                .env("PATH", "/usr/bin:/bin")
                .output()
                .expect("anchored npm command should start");

            assert!(
                output.status.success(),
                "anchored npm command failed: {}",
                decode_command_output(&output.stderr)
            );
            assert_eq!(
                std::fs::read_to_string(marker).expect("sibling node should leave a marker"),
                "sibling-node"
            );
        }

        #[test]
        fn claude_native_path_with_space_is_quoted() {
            // claude 分支同样要 POSIX 引号包裹含空格的 bin_path,
            // 否则 `/Users/my name/.local/bin/claude update` 会被 shell 拆词。
            let cmd = anchored_command_from_paths(
                "claude",
                "/Users/my name/.local/bin/claude",
                "/Users/my name/.local/share/claude/versions/2.1.146",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("'/Users/my name/.local/bin/claude' update")
            );
        }

        #[test]
        fn brew_path_with_space_is_quoted() {
            // brew 分支用 `<bin_path 同目录>/brew`,目录含空格时同样要引号包裹。
            let cmd = anchored_command_from_paths(
                "gemini",
                "/opt/my brew/bin/gemini",
                "/opt/my brew/Cellar/gemini-cli/0.13.0/libexec/lib/node_modules/@google/gemini-cli/dist/index.js",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("'/opt/my brew/bin/brew' upgrade gemini-cli")
            );
        }

        #[test]
        fn brew_cask_path_with_space_is_quoted() {
            let cmd = anchored_command_from_paths(
                "codex",
                "/opt/my brew/bin/codex",
                "/opt/my brew/Caskroom/codex/0.146.0/bin/codex",
            );
            assert_eq!(
                cmd.as_deref(),
                Some("'/opt/my brew/bin/brew' upgrade --cask codex")
            );
        }

        #[test]
        fn brew_formula_extraction() {
            assert_eq!(
                brew_formula_from_path("/opt/homebrew/Cellar/gemini-cli/0.13.0/bin/gemini")
                    .as_deref(),
                Some("gemini-cli")
            );
            // node 全局包不在 Cellar 下 → 不是 formula。
            assert_eq!(
                brew_formula_from_path("/opt/homebrew/lib/node_modules/openclaw/openclaw.mjs"),
                None
            );
            assert_eq!(
                brew_formula_from_path("/Users/me/.nvm/versions/node/v22/lib/node_modules/x"),
                None
            );
            // Caskroom 不是 formula。
            assert_eq!(
                brew_formula_from_path("/opt/homebrew/Caskroom/codex/0.146.0/bin/codex"),
                None
            );
        }

        #[test]
        fn brew_cask_extraction() {
            assert_eq!(
                brew_cask_from_path("/opt/homebrew/Caskroom/codex/0.146.0/bin/codex").as_deref(),
                Some("codex")
            );
            // Intel prefix 不含 /homebrew/，仍能抽出 token。
            assert_eq!(
                brew_cask_from_path("/usr/local/Caskroom/codex/0.146.0/bin/codex").as_deref(),
                Some("codex")
            );
            // formula / npm 全局包都不是 cask。
            assert_eq!(
                brew_cask_from_path("/opt/homebrew/Cellar/codex/1.2.3/bin/codex"),
                None
            );
            assert_eq!(
                brew_cask_from_path("/opt/homebrew/lib/node_modules/@openai/codex/bin/codex.js"),
                None
            );
        }

        #[test]
        fn sibling_bin_returns_none_when_bin_path_has_no_directory() {
            // bin_path 不含 `/` → parent_dir 返回空 → sibling_bin 不能拼出绝对路径
            // → None,让上游 anchored_command_from_paths 整体退化为静态命令兜底,
            // 而不是悄悄拼出 `npm i -g <pkg>` 这种依赖 PATH 的指令(违背"必须绝对路径"
            // 不变量)。实际从 enumerate_tool_installations 走的 bin_path 都是绝对路径,
            // 这条防线不期望被触发,但闭合了 helper 与函数文档的语义一致。
            assert_eq!(sibling_bin("codex", "npm"), None);
            assert_eq!(sibling_bin("", "brew"), None);
            // 含 `/` 即可拼出绝对路径——这是常规路径。
            assert_eq!(
                sibling_bin("/opt/homebrew/bin/gemini", "brew").as_deref(),
                Some("/opt/homebrew/bin/brew")
            );
        }

        #[test]
        fn default_install_prefers_path_default() {
            let installs = vec![
                inst("/opt/homebrew/bin/openclaw", false),
                inst("/Users/me/.nvm/versions/node/v22/bin/openclaw", true),
            ];
            assert_eq!(
                default_install(&installs).map(|i| i.path.as_str()),
                Some("/Users/me/.nvm/versions/node/v22/bin/openclaw")
            );
        }

        #[test]
        fn default_install_falls_back_to_sole_entry() {
            let installs = vec![inst("/opt/homebrew/bin/gemini", false)];
            assert_eq!(
                default_install(&installs).map(|i| i.path.as_str()),
                Some("/opt/homebrew/bin/gemini")
            );
        }

        #[test]
        fn default_install_none_when_ambiguous() {
            let installs = vec![
                inst("/opt/homebrew/bin/openclaw", false),
                inst("/Users/me/.nvm/versions/node/v22/bin/openclaw", false),
            ];
            assert!(default_install(&installs).is_none());
        }

        #[test]
        fn codex_missing_platform_binary_self_heals_via_uninstall_install() {
            // 平台二进制缺失 → `codex --version` 报 "Missing optional dependency" 退出非 0
            // → enumerate 标记 runnable=false。此状态下普通 `npm i -g @latest` 是 no-op 修不好,
            // 升级路径改用 uninstall+install 重装补回平台二进制（`|| true` 让 uninstall 在
            // set -e 下对半损坏包返回非 0 时仍继续 install）。
            let mut broken = inst("/Users/me/.nvm/versions/node/v22.14.0/bin/codex", true);
            broken.runnable = false;
            broken.exited_with_error = true;
            assert_eq!(
                installs_anchored_command("codex", &[broken]).as_deref(),
                Some("PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm uninstall -g @openai/codex || true; PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm i -g @openai/codex@latest")
            );
        }

        #[test]
        fn codex_probe_timeout_uses_plain_npm_not_uninstall() {
            // `--version` 超时被击杀 / 启动失败：runnable=false 但不是命令自己报错，不能当成
            // 装坏了。走卸载重装时，一旦装失败旧版就没了，用户看到「未安装」；普通
            // `npm i -g` 失败 npm 会回滚、旧版还在。
            let mut probe_failed = inst("/Users/me/.nvm/versions/node/v22.14.0/bin/codex", true);
            probe_failed.runnable = false;
            probe_failed.error = Some("Command timed out after 10s".to_string());
            let cmd = installs_anchored_command("codex", &[probe_failed]);
            assert_eq!(
                cmd.as_deref(),
                Some("PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm i -g @openai/codex@latest")
            );
            assert!(!cmd.unwrap().contains("uninstall"));
        }

        #[test]
        fn codex_runnable_uses_plain_npm_not_self_heal() {
            // 正常（runnable=true）的 codex 升级：锚定 npm，既不重装、也不跑会假成功
            // 掩盖损坏的 `codex update`。
            let healthy = inst("/Users/me/.nvm/versions/node/v22.14.0/bin/codex", true);
            let cmd = installs_anchored_command("codex", &[healthy]);
            assert_eq!(
                cmd.as_deref(),
                Some("PATH='/Users/me/.nvm/versions/node/v22.14.0/bin':\"$PATH\" /Users/me/.nvm/versions/node/v22.14.0/bin/npm i -g @openai/codex@latest")
            );
            assert!(!cmd.unwrap().contains("uninstall"));
        }

        #[test]
        fn codex_broken_homebrew_formula_uses_brew_not_npm_repair() {
            // brew formula 装的坏 codex（real 在 Cellar）：自愈门控必须收窄放行，回落到
            // `brew upgrade codex`——若误走 npm 重装，npm 够不到 Cellar 那份、反而旁路
            // 装第二份 npm 全局 codex 制造双安装。
            let broken = ToolInstallation {
                path: "/opt/homebrew/bin/codex".to_string(),
                version: None,
                runnable: false,
                error: None,
                exited_with_error: true,
                source: "homebrew".to_string(),
                is_path_default: true,
                real: std::path::PathBuf::from("/opt/homebrew/Cellar/codex/1.2.3/bin/codex"),
            };
            assert_eq!(
                installs_anchored_command("codex", &[broken]).as_deref(),
                Some("/opt/homebrew/bin/brew upgrade codex")
            );
        }

        #[test]
        fn codex_broken_homebrew_cask_uses_brew_not_npm_repair() {
            // brew cask 装的坏 codex（real 在 Caskroom）：与 formula 同理，必须回落到
            // `brew upgrade --cask`。误走 npm uninstall+install 会撞 EEXIST，或旁路
            // 装第二份 npm 全局包争抢同一个 `/opt/homebrew/bin/codex`。
            let broken = ToolInstallation {
                path: "/opt/homebrew/bin/codex".to_string(),
                version: None,
                runnable: false,
                error: None,
                exited_with_error: true,
                source: "homebrew".to_string(),
                is_path_default: true,
                real: std::path::PathBuf::from("/opt/homebrew/Caskroom/codex/0.146.0/bin/codex"),
            };
            assert_eq!(
                installs_anchored_command("codex", &[broken]).as_deref(),
                Some("/opt/homebrew/bin/brew upgrade --cask codex")
            );
        }

        #[test]
        fn codex_broken_volta_uses_volta_install_not_npm_repair() {
            // volta 装的坏 codex：回落到 `volta install`，不走 npm 重装。
            let mut broken = inst("/Users/me/.volta/bin/codex", true);
            broken.runnable = false;
            broken.exited_with_error = true;
            assert_eq!(
                installs_anchored_command("codex", &[broken]).as_deref(),
                Some("/Users/me/.volta/bin/volta install @openai/codex")
            );
        }

        #[test]
        fn codex_broken_bun_uses_bun_add_not_phantom_npm() {
            // bun 装的坏 codex：回落到 `bun add`，且**绝不**拼出 `~/.bun/bin/npm`
            // （bun 目录下没有 npm，那条路径不存在、执行会直接失败）。
            let mut broken = inst("/Users/me/.bun/bin/codex", true);
            broken.runnable = false;
            broken.exited_with_error = true;
            let cmd = installs_anchored_command("codex", &[broken]);
            assert_eq!(
                cmd.as_deref(),
                Some("/Users/me/.bun/bin/bun add -g @openai/codex@latest")
            );
            assert!(!cmd.unwrap().contains("npm"));
        }

        #[test]
        fn codex_standalone_reruns_official_installer_in_place() {
            // #7650:官方独立安装器装的 codex 此前被判 system 来源、锚定落空,退回裸
            // `npm i -g` 另装一份 npm 版。现在带 CODEX_INSTALL_DIR / CODEX_HOME 重跑官方
            // installer;不用 `codex update`——它断网时 exit 0 假成功。
            let cmd = anchored_command_from_paths(
                "codex",
                "/Users/me/.local/bin/codex",
                "/Users/me/.codex/packages/standalone/releases/0.157.0-aarch64-apple-darwin/bin/codex",
            )
            .unwrap();
            assert_eq!(
                cmd,
                format!(
                    "CODEX_NON_INTERACTIVE=1 CODEX_INSTALL_DIR='/Users/me/.local/bin' CODEX_HOME='/Users/me/.codex' {CODEX_INSTALL_UNIX}"
                )
            );
            assert!(!cmd.contains("npm"), "npm must not be used: {cmd}");
            assert!(
                !cmd.contains("codex update"),
                "codex update fakes success: {cmd}"
            );
        }

        #[test]
        fn codex_standalone_custom_dirs_are_written_back() {
            // 自定义 CODEX_INSTALL_DIR / CODEX_HOME 装的一处:不显式传回去,installer 会装进
            // 默认的 ~/.local/bin 与 ~/.codex,命令行实际在用的那份原地不动。
            let cmd = anchored_command_from_paths(
                "codex",
                "/opt/tools/codex bin/codex",
                "/data/codex-home/packages/standalone/releases/0.157.0-x86_64-unknown-linux-musl/bin/codex",
            );
            assert_eq!(
                cmd,
                Some(format!(
                    "CODEX_NON_INTERACTIVE=1 CODEX_INSTALL_DIR='/opt/tools/codex bin' CODEX_HOME='/data/codex-home' {CODEX_INSTALL_UNIX}"
                ))
            );
        }

        #[test]
        fn codex_broken_standalone_reinstalls_via_installer_not_npm_repair() {
            // 跑不起来的独立安装版:npm 自愈门控只放行 nvm/fnm/mise/homebrew,这里交回锚定,
            // 由官方 installer 重装同一处。
            let broken = ToolInstallation {
                path: "/Users/me/.local/bin/codex".to_string(),
                version: None,
                runnable: false,
                error: None,
                exited_with_error: true,
                source: "system".to_string(),
                is_path_default: true,
                real: std::path::PathBuf::from(
                    "/Users/me/.codex/packages/standalone/releases/0.157.0-aarch64-apple-darwin/bin/codex",
                ),
            };
            let cmd = installs_anchored_command("codex", &[broken]).unwrap();
            assert!(cmd.ends_with(CODEX_INSTALL_UNIX), "{cmd}");
            assert!(!cmd.contains("npm"), "{cmd}");
        }

        #[test]
        fn codex_broken_standalone_in_node_manager_dir_skips_npm_repair() {
            // CODEX_INSTALL_DIR 指到 Homebrew / nvm 的 bin 目录时,入口路径会被判成
            // homebrew / nvm 来源;npm 自愈门控只看入口来源的话会抢先返回 npm 重装,
            // 把独立安装版换成 npm 版。真身仍在 standalone 发布包里,必须交回 installer。
            for bin in [
                "/opt/homebrew/bin/codex",
                "/Users/me/.nvm/versions/node/v22.19.0/bin/codex",
            ] {
                let broken = ToolInstallation {
                    path: bin.to_string(),
                    version: None,
                    runnable: false,
                    error: None,
                    exited_with_error: true,
                    source: infer_install_source(Path::new(bin)).to_string(),
                    is_path_default: true,
                    real: std::path::PathBuf::from(
                        "/Users/me/.codex/packages/standalone/releases/0.157.0-aarch64-apple-darwin/bin/codex",
                    ),
                };
                let cmd = installs_anchored_command("codex", &[broken]).unwrap();
                assert!(cmd.ends_with(CODEX_INSTALL_UNIX), "{bin}: {cmd}");
                assert!(!cmd.contains("npm"), "{bin}: {cmd}");
            }
        }

        #[test]
        fn claude_homebrew_cask_uses_brew_not_self_update_or_npm() {
            // cask 归 brew 管:与 formula 一样不先跑 `claude update`,也不挂 npm fallback。
            let cmd = anchored_command_from_paths(
                "claude",
                "/opt/homebrew/bin/claude",
                "/opt/homebrew/Caskroom/claude-code/2.1.274/claude",
            )
            .unwrap();
            assert_eq!(cmd, "/opt/homebrew/bin/brew upgrade --cask claude-code");
            assert!(!cmd.contains("claude update"));
            assert!(!cmd.contains("npm"));
        }

        #[test]
        fn first_abs_path_line_skips_shell_noise() {
            // 交互式 .zshrc 先打印欢迎语（如 powerlevel10k / 自定义提示），
            // command -v 的真实路径在其后 → 跳过噪音取真路径。
            assert_eq!(
                first_abs_path_line("🚀 Welcome back!\n/Users/me/.local/bin/claude\n"),
                Some("/Users/me/.local/bin/claude")
            );
            // 无噪音时取第一行。
            assert_eq!(
                first_abs_path_line("/opt/homebrew/bin/gemini\n"),
                Some("/opt/homebrew/bin/gemini")
            );
            // 输出里没有任何绝对路径 → None。
            assert_eq!(first_abs_path_line("welcome\nbye\n"), None);
        }

        #[test]
        fn path_line_from_env_output_survives_shell_noise() {
            // `$SHELL -lic /usr/bin/env` 的 stdout 前面可能有交互式 rc 的欢迎语。
            let raw = "🚀 Welcome back, Jason!\nSHELL=/bin/zsh\nPATH=/opt/homebrew/bin:/usr/bin\nHOME=/Users/me\n";
            assert_eq!(
                path_line_from_env_output(raw),
                Some("/opt/homebrew/bin:/usr/bin")
            );
            // 多行值的环境变量里恰好有一行以 `PATH=` 开头时，「值须以 / 开头」把它筛掉。
            let poisoned = "SOME_SCRIPT=line1\nPATH=not-a-path\nPATH=/usr/bin:/bin\n";
            assert_eq!(path_line_from_env_output(poisoned), Some("/usr/bin:/bin"));
            // 完全没有 PATH 行 → None，调用方保持不注入。
            assert_eq!(path_line_from_env_output("HOME=/Users/me\n"), None);
        }

        #[test]
        fn merge_path_segments_dedupes_preserving_login_order() {
            // 登录 shell 的段全部在前且保序；继承 PATH 里的新段追加在后。
            assert_eq!(
                merge_path_segments(
                    "/Users/me/.nvm/versions/node/v22/bin:/usr/bin:/bin",
                    "/usr/bin:/bin:/usr/sbin"
                ),
                "/Users/me/.nvm/versions/node/v22/bin:/usr/bin:/bin:/usr/sbin"
            );
            // 空段（`a::b` 在 POSIX 下意为当前目录）不该被注入。
            assert_eq!(merge_path_segments("/usr/bin::/bin", ""), "/usr/bin:/bin");
        }

        #[test]
        fn is_conflicting_thresholds() {
            let make = |version: Option<&str>, runnable: bool| ToolInstallation {
                path: "/x".to_string(),
                version: version.map(str::to_string),
                runnable,
                error: None,
                exited_with_error: false,
                source: "nvm".to_string(),
                is_path_default: false,
                real: std::path::PathBuf::from("/x"),
            };
            // 单处 → 不冲突。
            assert!(!is_conflicting(&[make(Some("1.0.0"), true)]));
            // 两处同版本、都能跑 → 不冲突（同版本装两遍不打扰）。
            assert!(!is_conflicting(&[
                make(Some("1.0.0"), true),
                make(Some("1.0.0"), true)
            ]));
            // 版本分歧 → 冲突。
            assert!(is_conflicting(&[
                make(Some("1.0.0"), true),
                make(Some("2.0.0"), true)
            ]));
            // 同版本但运行态混合（一个能跑、一个跑不起来）→ 冲突。
            assert!(is_conflicting(&[
                make(Some("1.0.0"), true),
                make(Some("1.0.0"), false)
            ]));
        }
    }

    /// install 端的"上游推荐 || npm 兜底"短路链:把工具→官方安装方式这一上游事实
    /// 固化为回归断言。任何方案改动若打破短路链结构或 URL,都会被这些用例拦下。
    #[cfg(not(target_os = "windows"))]
    mod install_strategy {
        use super::super::*;

        #[test]
        fn claude_install_prefers_native_with_npm_fallback() {
            // Anthropic 现在主推 native installer(claude.ai/install.sh),
            // 网络不通时短路到 npm 仍能装上;两段都得在,顺序也得对。
            let cmd = install_command_for("claude");
            assert!(
                cmd.contains("https://claude.ai/install.sh"),
                "should include official installer URL: {cmd}"
            );
            assert!(
                cmd.contains("@anthropic-ai/claude-code@latest"),
                "should keep npm package as fallback: {cmd}"
            );
            let parts: Vec<&str> = cmd.split("||").collect();
            assert_eq!(parts.len(), 2, "should be a two-step short-circuit chain");
            assert!(parts[0].contains("install.sh"), "native first: {cmd}");
            assert!(
                !parts[0].contains('|'),
                "native installer should avoid pipe: {cmd}"
            );
            assert!(parts[1].contains("npm i -g"), "npm second: {cmd}");
        }

        #[test]
        fn opencode_install_prefers_native_with_npm_fallback() {
            // SST 自家 install.sh 与 claude 同形态:bash 脚本、网络下载、装到 ~/.opencode/bin。
            let cmd = install_command_for("opencode");
            assert!(
                cmd.contains("https://opencode.ai/install"),
                "should include official installer URL: {cmd}"
            );
            assert!(
                cmd.contains("opencode-ai@latest"),
                "should keep npm package as fallback: {cmd}"
            );
            assert!(cmd.contains("||"), "should chain fallback: {cmd}");
            assert!(
                !cmd.split("||").next().unwrap_or_default().contains('|'),
                "native installer should avoid pipe: {cmd}"
            );
        }

        #[test]
        fn codex_install_keeps_static_npm() {
            // OpenAI 暂无独立 native installer,保持原裸 npm,不引入兜底链(无东西可兜底)。
            let cmd = install_command_for("codex");
            assert_eq!(cmd, "npm i -g @openai/codex@latest");
            assert!(!cmd.contains("||"));
        }

        #[test]
        fn gemini_install_keeps_static_npm() {
            // Google 文档同时支持 brew/npm,但本表保持与 update fallback 一致的 npm。
            // 用户若已装 brew gemini-cli,update 路径的锚定会识别 formula → brew upgrade,
            // 所以 install 端不强行替用户决策"用 brew 还是 npm"。
            let cmd = install_command_for("gemini");
            assert_eq!(cmd, "npm i -g @google/gemini-cli@latest");
        }

        #[test]
        fn grok_install_prefers_native_with_npm_fallback() {
            let cmd = install_command_for("grok");
            assert!(
                cmd.contains("https://x.ai/cli/install.sh"),
                "should include official installer URL: {cmd}"
            );
            assert!(
                cmd.contains("@xai-official/grok@latest"),
                "should keep npm package as fallback: {cmd}"
            );
            let parts: Vec<&str> = cmd.split("||").collect();
            assert_eq!(parts.len(), 2, "should be a two-step short-circuit chain");
            assert!(parts[0].contains("install.sh"), "native first: {cmd}");
            assert!(
                !parts[0].contains('|'),
                "native installer should avoid pipe: {cmd}"
            );
            // npm 12 拦截 postinstall 时升级 exit 0 却留在旧版（#8863）。
            assert_eq!(
                parts[1].trim(),
                npm_install_command_for("grok").unwrap(),
                "npm second: {cmd}"
            );
        }

        #[test]
        fn mcode_install_prefers_official_installer_with_npm_fallback() {
            let cmd = install_command_for("mcode");
            let (installer, fallback) = cmd
                .split_once(" || ")
                .expect("install should chain an npm fallback");
            assert!(
                installer.contains("https://filecdn.minimax.chat/public/install.sh"),
                "official installer first: {cmd}"
            );
            assert!(
                !installer.contains('|'),
                "native installer should avoid pipe: {cmd}"
            );
            assert_eq!(Some(fallback), npm_install_command_for("mcode"));
        }

        #[test]
        fn openclaw_install_keeps_static_npm() {
            let cmd = install_command_for("openclaw");
            assert_eq!(cmd, "npm i -g openclaw@latest");
        }

        #[test]
        fn pi_install_uses_the_verified_pinned_package() {
            let cmd = install_command_for("pi");
            assert_eq!(cmd, "npm i -g @earendil-works/pi-coding-agent@latest");
            assert!(!cmd.contains("||"));
        }

        #[test]
        fn update_fallbacks_use_official_cli_only_when_supported() {
            assert_eq!(
                static_fallback_command("claude"),
                "claude update || npm i -g @anthropic-ai/claude-code@latest --ignore-scripts=false --include=optional --allow-scripts=@anthropic-ai/claude-code"
            );
            assert_eq!(
                static_fallback_command("codex"),
                "npm i -g @openai/codex@latest"
            );
            assert!(!static_fallback_command("codex").contains("codex update"));
            assert_eq!(
                static_fallback_command("gemini"),
                "npm i -g @google/gemini-cli@latest"
            );
            assert!(!static_fallback_command("gemini").contains("gemini update"));
            assert_eq!(
                static_fallback_command("grok"),
                "npm i -g @xai-official/grok@latest --ignore-scripts=false --include=optional --allow-scripts=@xai-official/grok"
            );
            assert!(!static_fallback_command("grok").contains("grok update"));
            assert_eq!(
                static_fallback_command("opencode"),
                "opencode upgrade || npm i -g opencode-ai@latest --ignore-scripts=false --include=optional --allow-scripts=opencode-ai"
            );
            assert_eq!(
                static_fallback_command("openclaw"),
                "openclaw update --yes || npm i -g openclaw@latest"
            );
            assert_eq!(
                static_fallback_command("pi"),
                "npm i -g @earendil-works/pi-coding-agent@latest"
            );
            assert!(!static_fallback_command("pi").contains("pi update"));
            assert_eq!(
                Some(static_fallback_command("mcode").as_str()),
                npm_install_command_for("mcode")
            );
            assert!(!static_fallback_command("mcode").contains("mcode update"));
        }

        #[test]
        fn hermes_install_uses_official_installer() {
            // Hermes 官方 installer 会处理 Python 3.11+/uv 等运行时;不要再从 cc-switch
            // 里走 `python3 || python` pip 链。
            let cmd = install_command_for("hermes");
            assert!(
                cmd.starts_with("bash -c 'tmp=$(mktemp) && curl -fsSL ")
                    && cmd.contains("install.sh -o $tmp && bash $tmp"),
                "should use official installer: {cmd}"
            );
            assert!(
                !cmd.contains('|') && !cmd.contains("python") && !cmd.contains("pip"),
                "should not depend on pipefail or system Python/pip: {cmd}"
            );
        }

        #[test]
        fn agy_installs_with_official_script_and_updates_without_reinstalling() {
            let cmd = install_command_for("agy");
            assert!(
                cmd.contains("curl -fsSL https://antigravity.google/cli/install.sh -o $tmp")
                    && !cmd.contains('|')
                    && !cmd.contains("npm"),
                "should use only the official installer: {cmd}"
            );
            // The installer exits 0 when agy already exists, so it must not back up `agy update`.
            assert_eq!(static_fallback_command("agy"), "agy update");
        }

        #[test]
        fn hermes_update_fallback_uses_cli_update_then_installer() {
            // 锚定失败时也不回退 pip:先让 PATH 上的 hermes 自更新,找不到/失败再跑官方
            // installer。这样 pyenv 的 `python` shim 不会参与错误路径。
            let cmd = static_fallback_command("hermes");
            assert!(
                cmd.starts_with("hermes update || bash -c 'tmp=$(mktemp) && curl -fsSL "),
                "should try CLI update before official installer: {cmd}"
            );
            let fallback = cmd
                .split_once("||")
                .map(|(_, fallback)| fallback)
                .expect("update should include installer fallback");
            assert!(
                !fallback.contains('|') && !cmd.contains("python") && !cmd.contains("pip"),
                "should not depend on pipefail or system Python/pip: {cmd}"
            );
        }
    }

    #[cfg(target_os = "windows")]
    mod wsl_helpers {
        use super::super::*;

        #[test]
        fn test_is_valid_shell() {
            assert!(is_valid_shell("bash"));
            assert!(is_valid_shell("zsh"));
            assert!(is_valid_shell("sh"));
            assert!(is_valid_shell("fish"));
            assert!(is_valid_shell("dash"));
            assert!(is_valid_shell("/usr/bin/bash"));
            assert!(is_valid_shell("/bin/zsh"));
            assert!(!is_valid_shell("powershell"));
            assert!(!is_valid_shell("cmd"));
            assert!(!is_valid_shell(""));
        }

        #[test]
        fn test_is_valid_shell_flag() {
            assert!(is_valid_shell_flag("-c"));
            assert!(is_valid_shell_flag("-lc"));
            assert!(is_valid_shell_flag("-lic"));
            assert!(!is_valid_shell_flag("-x"));
            assert!(!is_valid_shell_flag(""));
            assert!(!is_valid_shell_flag("--login"));
        }

        #[test]
        fn test_default_flag_for_shell() {
            assert_eq!(default_flag_for_shell("sh"), "-c");
            assert_eq!(default_flag_for_shell("dash"), "-c");
            assert_eq!(default_flag_for_shell("/bin/dash"), "-c");
            assert_eq!(default_flag_for_shell("fish"), "-lc");
            assert_eq!(default_flag_for_shell("bash"), "-lic");
            assert_eq!(default_flag_for_shell("zsh"), "-lic");
            assert_eq!(default_flag_for_shell("/usr/bin/zsh"), "-lic");
        }

        #[test]
        fn test_is_valid_wsl_distro_name() {
            assert!(is_valid_wsl_distro_name("Ubuntu"));
            assert!(is_valid_wsl_distro_name("Ubuntu-22.04"));
            assert!(is_valid_wsl_distro_name("my_distro"));
            assert!(!is_valid_wsl_distro_name(""));
            assert!(!is_valid_wsl_distro_name("distro with spaces"));
            assert!(!is_valid_wsl_distro_name(&"a".repeat(65)));
        }
    }

    #[test]
    fn opencode_extra_search_paths_includes_install_and_fallback_dirs() {
        let home = PathBuf::from("/home/tester");
        let install_dir = Some(std::ffi::OsString::from("/custom/opencode/bin"));
        let xdg_bin_dir = Some(std::ffi::OsString::from("/xdg/bin"));
        let gopath =
            std::env::join_paths([PathBuf::from("/go/path1"), PathBuf::from("/go/path2")]).ok();

        let paths = opencode_extra_search_paths(&home, install_dir, xdg_bin_dir, gopath);

        assert_eq!(paths[0], PathBuf::from("/custom/opencode/bin"));
        assert_eq!(paths[1], PathBuf::from("/xdg/bin"));
        assert!(paths.contains(&PathBuf::from("/home/tester/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/.opencode/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/.bun/bin")));
        assert!(paths.contains(&PathBuf::from("/home/tester/go/bin")));
        assert!(paths.contains(&PathBuf::from("/go/path1/bin")));
        assert!(paths.contains(&PathBuf::from("/go/path2/bin")));
    }

    #[test]
    fn opencode_extra_search_paths_deduplicates_repeated_entries() {
        let home = PathBuf::from("/home/tester");
        let same_dir = Some(std::ffi::OsString::from("/same/path"));

        let paths = opencode_extra_search_paths(&home, same_dir.clone(), same_dir, None);

        let count = paths
            .iter()
            .filter(|path| path.as_path() == Path::new("/same/path"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn opencode_extra_search_paths_deduplicates_bun_default_dir() {
        let home = PathBuf::from("/home/tester");
        let paths = opencode_extra_search_paths(&home, None, None, None);

        let count = paths
            .iter()
            .filter(|path| path.as_path() == Path::new("/home/tester/.bun/bin"))
            .count();
        assert_eq!(count, 1);
    }

    #[test]
    fn grok_extra_search_paths_prefers_override_then_default_native_dir() {
        let home = PathBuf::from("/home/tester");
        let paths =
            grok_extra_search_paths(&home, Some(std::ffi::OsString::from("/custom/grok/bin")));

        assert_eq!(paths[0], PathBuf::from("/custom/grok/bin"));
        assert_eq!(paths[1], PathBuf::from("/home/tester/.grok/bin"));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn mcode_extra_search_paths_use_installer_bin_dir() {
        let home = PathBuf::from("/home/tester");
        let paths = mcode_extra_search_paths(&home, Some(std::ffi::OsString::from("/opt/mcode")));
        assert_eq!(
            paths,
            vec![
                PathBuf::from("/opt/mcode/bin"),
                PathBuf::from("/home/tester/.minimax-code/bin"),
            ]
        );
        // 空的 MCODE_INSTALL_DIR 视同未设置（installer 同样回退默认目录）。
        let paths = mcode_extra_search_paths(&home, Some(std::ffi::OsString::new()));
        assert_eq!(paths, vec![PathBuf::from("/home/tester/.minimax-code/bin")]);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn mcode_extra_search_paths_use_installer_root_on_windows() {
        // Windows installer 把 mcode.cmd / mcode.ps1 放在安装根目录，不在 bin 下。
        let home = PathBuf::from("C:\\Users\\tester");
        let paths = mcode_extra_search_paths(&home, None);
        assert_eq!(paths, vec![home.join(".minimax-code")]);
    }

    #[test]
    fn grok_extra_search_paths_deduplicates_default_override() {
        let home = PathBuf::from("/home/tester");
        let paths = grok_extra_search_paths(
            &home,
            Some(std::ffi::OsString::from("/home/tester/.grok/bin")),
        );

        assert_eq!(paths, vec![PathBuf::from("/home/tester/.grok/bin")]);
    }

    #[test]
    fn unique_search_paths_preserve_order_and_skip_empty_entries() {
        let first = PathBuf::from("first/bin");
        let second = PathBuf::from("second/bin");
        let third = PathBuf::from("third/bin");
        let mut paths = IndexSet::new();

        for path in [
            PathBuf::new(),
            first.clone(),
            second.clone(),
            first.clone(),
            PathBuf::new(),
            third.clone(),
            second.clone(),
        ] {
            push_unique_path(&mut paths, path);
        }

        assert_eq!(
            paths.into_iter().collect::<Vec<_>>(),
            vec![first, second, third]
        );
    }

    #[test]
    fn unique_search_paths_keep_path_equality_and_first_spelling() {
        let first = PathBuf::from("node//bin");
        let equivalent = PathBuf::from("node/./bin");
        let parent = PathBuf::from("node/../node/bin");
        let different_case = PathBuf::from("Node/bin");
        assert_eq!(first, equivalent);
        let mut paths = IndexSet::new();

        for path in [
            first.clone(),
            equivalent,
            parent.clone(),
            different_case.clone(),
        ] {
            push_unique_path(&mut paths, path);
        }

        assert_eq!(paths[0].as_os_str(), first.as_os_str());
        assert_eq!(
            paths.into_iter().collect::<Vec<_>>(),
            vec![first, parent, different_case]
        );
    }

    #[test]
    fn unique_search_paths_handle_many_multishell_paths_and_late_duplicates() {
        let expected: Vec<_> = (0..4096)
            .map(|i| PathBuf::from(format!("fnm_multishells/{i}/bin")))
            .collect();
        let mut paths = IndexSet::new();

        for path in expected.iter().chain(expected.iter().rev()) {
            push_unique_path(&mut paths, path.clone());
        }

        assert_eq!(paths.into_iter().collect::<Vec<_>>(), expected);
    }

    #[cfg(unix)]
    #[test]
    fn unique_search_paths_preserve_non_utf8_paths() {
        use std::os::unix::ffi::OsStringExt;

        let first = PathBuf::from(std::ffi::OsString::from_vec(b"node-\xff/bin".to_vec()));
        let second = PathBuf::from(std::ffi::OsString::from_vec(b"node-\xfe/bin".to_vec()));
        let mut paths = IndexSet::new();
        for path in [first.clone(), second.clone(), first.clone()] {
            push_unique_path(&mut paths, path);
        }

        assert_eq!(paths.into_iter().collect::<Vec<_>>(), vec![first, second]);
    }

    #[test]
    fn cli_path_env_search_paths_include_path_entries_and_dedupe() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::create_dir_all(&first).expect("first dir should be created");
        std::fs::create_dir_all(&second).expect("second dir should be created");

        let path_env = std::env::join_paths([first.clone(), second.clone(), first.clone()])
            .expect("test path env should be joinable");
        let mut paths = IndexSet::from([first.clone()]);

        extend_from_cli_path_env(&mut paths, Some(path_env));

        assert!(paths.contains(&second));
        assert_eq!(paths.iter().filter(|path| *path == &first).count(), 1);
    }

    #[test]
    fn child_search_paths_include_existing_children_with_suffix() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let base = temp.path().join("node");
        let bin = base.join("25.8.0").join("bin");
        std::fs::create_dir_all(&bin).expect("version bin should be created");

        let mut paths = IndexSet::new();
        extend_existing_child_search_paths(&mut paths, &base, Some("bin"));

        assert!(paths.contains(&bin));
    }

    #[test]
    fn env_child_dir_appends_child_and_dedupes() {
        let base = std::ffi::OsString::from("/custom/toolchain");
        let mut paths = IndexSet::new();

        push_env_child_dir(&mut paths, Some(base.clone()), "bin");
        push_env_child_dir(&mut paths, Some(base), "bin");

        assert_eq!(
            paths.into_iter().collect::<Vec<_>>(),
            vec![PathBuf::from("/custom/toolchain").join("bin")]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn cli_path_env_skips_windows_apps_alias_dir() {
        assert!(is_windows_app_execution_alias_dir(Path::new(
            r"C:\Users\tester\AppData\Local\Microsoft\WindowsApps"
        )));
        assert!(!is_windows_app_execution_alias_dir(Path::new(
            r"C:\Users\tester\AppData\Roaming\npm"
        )));
    }

    #[test]
    fn fnm_default_alias_dir_candidates_follow_fnm_base_dir_order() {
        let home = Path::new("/home/tester");
        let data_dir = std::path::PathBuf::from("/home/tester/AppData/Roaming");
        let alias = |base: &str| {
            std::path::PathBuf::from(base)
                .join("aliases")
                .join("default")
        };

        assert_eq!(
            fnm_default_alias_dir_candidates(Some("/opt/fnm".into()), home, Some(data_dir.clone())),
            vec![
                alias("/opt/fnm"),
                alias("/home/tester/.fnm"),
                alias("/home/tester/AppData/Roaming/fnm"),
            ]
        );
        // Empty FNM_DIR / home are ignored instead of becoming relative paths.
        assert_eq!(
            fnm_default_alias_dir_candidates(Some("".into()), Path::new(""), Some(data_dir)),
            vec![alias("/home/tester/AppData/Roaming/fnm")]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn merge_path_segments_win_preserves_order_and_dedupes_case_insensitively() {
        let merged = merge_path_segments_win(&[
            r"C:\a;C:\B;%SystemRoot%\system32",
            r"C:\b;C:\a", // dup of C:\a (case-insensitive) and C:\B
            "",
        ]);
        assert_eq!(merged, r"C:\a;C:\B;%SystemRoot%\system32");
    }

    #[cfg(unix)]
    #[test]
    fn prepend_search_dir_to_path_preserves_non_utf8_bytes() {
        use std::os::unix::ffi::{OsStrExt, OsStringExt};

        let current = std::ffi::OsString::from_vec(b"/usr/bin:/tmp/\xff/bin".to_vec());
        let combined = prepend_search_dir_to_path(Path::new("/candidate/bin"), &current);

        assert_eq!(
            combined.as_os_str().as_bytes(),
            b"/candidate/bin:/usr/bin:/tmp/\xff/bin"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn expand_env_chars_preserves_unknown_vars_and_plain_text() {
        // No percent signs -> verbatim.
        assert_eq!(
            expand_env_chars(r"C:\Program Files\nodejs"),
            r"C:\Program Files\nodejs"
        );
        // Undefined variable is preserved verbatim (nothing dropped).
        assert_eq!(
            expand_env_chars(r"D:\npm-global\%DEFINITELY_NOT_A_REAL_VAR_xyz%\bin"),
            r"D:\npm-global\%DEFINITELY_NOT_A_REAL_VAR_xyz%\bin"
        );
        // Empty percent pair is not treated as a variable name.
        assert_eq!(expand_env_chars(r"C:\path\%%\tail"), r"C:\path\%%\tail");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn expand_env_chars_recovers_missing_user_variables_and_nested_references() {
        let process = |name: &str| match name.to_ascii_uppercase().as_str() {
            "USERPROFILE" => Some(r"C:\Users\tester".into()),
            "OVERRIDE" => Some(r"D:\runtime".into()),
            _ => None,
        };
        let registry = |name: &str| match name.to_ascii_uppercase().as_str() {
            "PNPM_HOME" => Some(r"%LOCALAPPDATA%\pnpm".into()),
            "LOCALAPPDATA" => Some(r"%USERPROFILE%\AppData\Local".into()),
            "OVERRIDE" => Some(r"D:\registry".into()),
            "CYCLE_A" => Some("%CYCLE_B%".into()),
            "CYCLE_B" => Some("%cycle_a%".into()),
            _ => None,
        };
        let expanded = expand_env_chars_with_lookup(
            r"%pnpm_home%;%OVERRIDE%;%MISSING%;%CYCLE_A%;%PNPM_HOME%;unfinished%",
            &process,
            &registry,
            &mut Vec::new(),
        );
        assert_eq!(
            expanded,
            r"C:\Users\tester\AppData\Local\pnpm;D:\runtime;%MISSING%;%cycle_a%;C:\Users\tester\AppData\Local\pnpm;unfinished%"
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn effective_path_keeps_process_entries_with_non_unicode_path() {
        use std::os::windows::ffi::OsStringExt;

        const CHILD: &str = "CC_SWITCH_TEST_NON_UNICODE_PATH";
        if std::env::var_os(CHILD).is_some() {
            assert!(std::env::var("PATH").is_err());
            let effective = effective_path_string();
            assert!(effective.starts_with(r"C:\cc-switch-path-test;"));
            assert!(effective.contains(r";D:\cc-switch-path-test"));
            return;
        }
        // Use a subprocess so other tests never observe a modified process PATH.
        let mut path: Vec<u16> = r"C:\cc-switch-path-test;C:\".encode_utf16().collect();
        path.push(0xD800); // Unpaired UTF-16 surrogate.
        path.extend(r";D:\cc-switch-path-test".encode_utf16());
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "commands::misc::tests::effective_path_keeps_process_entries_with_non_unicode_path",
                "--nocapture",
            ])
            .env(CHILD, "1")
            .env("PATH", std::ffi::OsString::from_wide(&path))
            .output()
            .unwrap();
        assert!(output.status.success(), "{output:?}");
        assert!(String::from_utf8_lossy(&output.stdout).contains("1 passed"));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn build_tool_search_paths_includes_standalone_installer_dirs() {
        // Non-npm installer locations must be scanned even when the process PATH
        // dropped them (regression guard for #6061 / #6278 / #6047).
        let local_data = dirs::data_local_dir().expect("LOCALAPPDATA should resolve");

        let codex_paths = build_tool_search_paths("codex");
        assert!(codex_paths.contains(
            &local_data
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
        ));

        let claude_paths = build_tool_search_paths("claude");
        assert!(claude_paths.contains(&local_data.join("Programs").join("claude")));

        // The standalone Codex dir is codex-specific; it must not pollute other tools.
        assert!(!build_tool_search_paths("gemini").contains(
            &local_data
                .join("Programs")
                .join("OpenAI")
                .join("Codex")
                .join("bin")
        ));
    }

    #[test]
    fn omp_search_paths_include_the_bun_global_bin() {
        // A GUI process does not inherit `~/.bun/bin`: without it a Bun global
        // omp is reported as not installed and updates lose their anchor.
        let home = dirs::home_dir().expect("home should resolve");
        let bun_bin = home.join(".bun").join("bin");
        assert!(build_tool_search_paths("omp").contains(&bun_bin));
        // Other Bun-installable tools keep their previous search set.
        assert!(!build_tool_search_paths("gemini").contains(&bun_bin));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_path_lookup_ignores_same_named_file_in_current_directory() {
        let current_dir = tempfile::tempdir().expect("current directory should be created");
        let path_dir = tempfile::tempdir().expect("PATH directory should be created");
        std::fs::write(current_dir.path().join("codex.cmd"), "@echo current\r\n")
            .expect("current-directory shim should be created");
        let expected = path_dir.path().join("codex.cmd");
        std::fs::write(&expected, "@echo path\r\n").expect("PATH shim should be created");

        let effective_path =
            std::env::join_paths([path_dir.path()]).expect("test PATH should join");
        let output = windows_path_lookup_command("codex", &effective_path)
            .current_dir(current_dir.path())
            .output()
            .expect("where.exe should execute");
        let stderr = decode_command_output(&output.stderr);
        let matches = decode_command_output(&output.stdout)
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .map(PathBuf::from)
            .collect::<Vec<_>>();

        assert!(output.status.success(), "where.exe failed: {stderr}");
        assert_eq!(matches.len(), 1);
        assert_eq!(
            std::fs::canonicalize(&matches[0]).expect("where.exe match should canonicalize"),
            std::fs::canonicalize(&expected).expect("expected PATH shim should canonicalize")
        );
    }

    #[test]
    fn mise_node_search_paths_include_shims_and_installed_node_bins() {
        let temp = tempfile::tempdir().expect("temp dir should be created");
        let home = temp.path();
        let node_bin = home
            .join(".local/share/mise/installs/node/25.8.0")
            .join("bin");
        std::fs::create_dir_all(&node_bin).expect("node bin should be created");

        let mut paths = IndexSet::new();
        extend_mise_node_search_paths(&mut paths, home);

        assert!(paths.contains(&home.join(".local/share/mise/shims")));
        assert!(paths.contains(&node_bin));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn mise_shim_target_resolves_shim_through_mise_which() {
        use std::os::unix::fs::PermissionsExt;

        let temp = tempfile::tempdir().expect("temp dir should be created");
        let root = temp.path();
        let node_bin = root.join("installs/node/22.12.0/bin");
        std::fs::create_dir_all(&node_bin).expect("node bin should be created");
        let target = node_bin.join("claude");
        std::fs::write(&target, "").expect("target should be written");

        let mise_dir = root.join("bin");
        std::fs::create_dir_all(&mise_dir).expect("mise dir should be created");
        let mise = mise_dir.join("mise");
        std::fs::write(
            &mise,
            format!(
                "#!/bin/sh\n[ \"$1\" = which ] && [ \"$2\" = claude ] && echo '{}'\n",
                target.display()
            ),
        )
        .expect("fake mise should be written");
        std::fs::set_permissions(&mise, std::fs::Permissions::from_mode(0o755))
            .expect("fake mise should be executable");

        let shims = root.join("shims");
        std::fs::create_dir_all(&shims).expect("shims dir should be created");
        let shim = shims.join("claude");
        std::os::unix::fs::symlink(&mise, &shim).expect("shim symlink should be created");
        let shim_real = std::fs::canonicalize(&shim).expect("shim should canonicalize");

        assert_eq!(
            mise_shim_target("claude", &shim, &shim_real, None),
            Some(target.clone())
        );
        // 不是 mise shim 的候选不去问 mise。
        let target_real = std::fs::canonicalize(&target).expect("target should canonicalize");
        assert_eq!(
            mise_shim_target("claude", &target, &target_real, None),
            None
        );
        // mise 说不出路径（工具没装在当前版本里）时不猜。
        assert_eq!(mise_shim_target("codex", &shim, &shim_real, None), None);

        // 枚举安装时 shim 换成真身，和 installs 目录那处去重成同一处（#9515）；
        // shim 模式下默认入口是 mise 本体，也换成真身。
        let mut path_default = Some(shim_real.clone());
        let (path, real) = resolve_mise_shim_install(
            "claude",
            shim.clone(),
            shim_real.clone(),
            &mut path_default,
            None,
        );
        assert_eq!(path, target);
        assert_eq!(real, target_real);
        assert_eq!(path_default, Some(target_real.clone()));
        // 解析不了的 shim 原样保留，默认入口不动。
        let mut path_default = Some(shim_real.clone());
        let (path, real) = resolve_mise_shim_install(
            "codex",
            shim.clone(),
            shim_real.clone(),
            &mut path_default,
            None,
        );
        assert_eq!(path, shim);
        assert_eq!(real, shim_real);
        assert_eq!(path_default, Some(shim_real));
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn tool_executable_candidates_non_windows_uses_plain_binary_name() {
        let dir = PathBuf::from("/usr/local/bin");
        let candidates = tool_executable_candidates("opencode", &dir);

        assert_eq!(candidates, vec![PathBuf::from("/usr/local/bin/opencode")]);
    }

    /// 文件在、却拿不到版本（起不来 / 没输出）算「装了但跑不起来」，不算没装：
    /// 安装位置列表照样列出这类文件，报没装会让应用页同一行既在「已安装」组又显示安装按钮。
    #[cfg(not(target_os = "windows"))]
    #[test]
    fn scan_cli_version_reports_present_but_silent_executable_as_broken() {
        use std::os::unix::fs::PermissionsExt;

        let root = tempfile::tempdir().expect("temp dir should be created");
        let make_dir = |name: &str, script: Option<&str>| {
            let dir = root.path().join(name);
            std::fs::create_dir_all(&dir).expect("dir should be created");
            let tool = dir.join("kimi");
            match script {
                Some(body) => {
                    std::fs::write(&tool, format!("#!/bin/sh\n{body}\n"))
                        .expect("script should be written");
                    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755))
                        .expect("script should be executable");
                }
                // 不可执行的文件：启动失败
                None => std::fs::write(&tool, "").expect("file should be written"),
            }
            dir
        };
        let silent = make_dir("silent", Some("exit 0"));
        let silent_failure = make_dir("silent-failure", Some("exit 3"));
        let not_executable = make_dir("not-executable", None);
        let loud_failure = make_dir("loud-failure", Some("echo boom >&2\nexit 1"));
        let working = make_dir("working", Some("echo 'kimi 1.2.3'"));
        let empty = root.path().join("empty");
        std::fs::create_dir_all(&empty).expect("dir should be created");
        // 与工具同名的目录不是安装
        let same_name_dir = root.path().join("same-name-dir");
        std::fs::create_dir_all(same_name_dir.join("kimi")).expect("dir should be created");

        let broken_detail = |paths: &[PathBuf]| match scan_cli_version_in("kimi", paths) {
            ShellProbe::FoundButFailed(detail) => detail,
            ShellProbe::Found(v) => panic!("expected broken, got version {v}"),
            ShellProbe::NotFound(e) => panic!("expected broken, got not found: {e}"),
        };

        assert!(broken_detail(std::slice::from_ref(&silent)).contains("printed nothing"));
        assert!(broken_detail(std::slice::from_ref(&silent_failure))
            .contains("exited with code 3 and printed nothing"));
        assert!(
            broken_detail(std::slice::from_ref(&not_executable)).starts_with("failed to launch")
        );
        // 具体报错优先于笼统诊断
        assert_eq!(broken_detail(&[silent.clone(), loud_failure]), "boom");
        // 别处有能跑的安装仍以它为准
        assert!(matches!(
            scan_cli_version_in("kimi", &[silent, working]),
            ShellProbe::Found(v) if v == "1.2.3"
        ));
        assert!(matches!(
            scan_cli_version_in("kimi", &[empty, same_name_dir]),
            ShellProbe::NotFound(_)
        ));
    }

    #[test]
    fn pathext_order_decides_whether_cmd_beats_exe() {
        assert!(!pathext_prefers_cmd_over_exe(None));
        assert!(!pathext_prefers_cmd_over_exe(Some(
            ".COM;.EXE;.BAT;.CMD;.VBS;.JS"
        )));
        assert!(pathext_prefers_cmd_over_exe(Some(".com;.cmd;.exe")));
        assert!(pathext_prefers_cmd_over_exe(Some(".COM;.CMD")));
        assert!(!pathext_prefers_cmd_over_exe(Some(".COM;.BAT")));
    }

    #[test]
    fn same_dir_cmd_is_shadowed_by_exe_sibling_issue_8839() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let cmd = dir.path().join("hermes.cmd");
        let exe = dir.path().join("hermes.exe");
        std::fs::write(&cmd, "").expect("cmd launcher should be created");

        let default_pathext = Some(".COM;.EXE;.BAT;.CMD");
        // A lone launcher is the install itself.
        assert!(!shadowed_by_pathext_sibling(&cmd, default_pathext));

        std::fs::write(&exe, "").expect("exe launcher should be created");
        assert!(shadowed_by_pathext_sibling(&cmd, default_pathext));
        assert!(!shadowed_by_pathext_sibling(&exe, default_pathext));

        // A PATHEXT that lists .CMD first makes the .cmd the one that runs.
        let cmd_first = Some(".COM;.CMD;.EXE");
        assert!(!shadowed_by_pathext_sibling(&cmd, cmd_first));
        assert!(shadowed_by_pathext_sibling(&exe, cmd_first));

        // A different tool's launcher in the same directory never shadows.
        let other = dir.path().join("codex.cmd");
        std::fs::write(&other, "").expect("other launcher should be created");
        assert!(!shadowed_by_pathext_sibling(&other, default_pathext));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_includes_cmd_exe_and_plain_name() {
        let dir = PathBuf::from("C:\\tools");
        let candidates = tool_executable_candidates("opencode", &dir);

        assert_eq!(
            candidates,
            vec![
                PathBuf::from("C:\\tools\\opencode.cmd"),
                PathBuf::from("C:\\tools\\opencode.exe"),
                PathBuf::from("C:\\tools\\opencode"),
            ]
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_prefers_opencode_desktop_cli_sidecar() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let gui = dir.path().join("opencode.exe");
        let sidecar = dir.path().join("opencode-cli.exe");
        std::fs::write(&gui, "").expect("desktop executable should be created");
        std::fs::write(&sidecar, "").expect("CLI sidecar should be created");

        let candidates = tool_executable_candidates("opencode", dir.path());

        assert_eq!(
            candidates,
            vec![dir.path().join("opencode.cmd"), sidecar.clone()]
        );
        assert!(!candidates.contains(&gui));
        assert_eq!(
            windows_preferred_tool_executable("opencode", &gui),
            Some(sidecar)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_prefers_opencode_resources_cli_sidecar() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let gui = dir.path().join("opencode.exe");
        let resources = dir.path().join("resources");
        let sidecar = resources.join("opencode-cli.exe");
        std::fs::create_dir(&resources).expect("resources dir should be created");
        std::fs::write(&gui, "").expect("desktop executable should be created");
        std::fs::write(resources.join("app.asar"), "").expect("app.asar should be created");
        std::fs::write(&sidecar, "").expect("CLI sidecar should be created");

        let candidates = tool_executable_candidates("opencode", dir.path());

        assert_eq!(
            candidates,
            vec![dir.path().join("opencode.cmd"), sidecar.clone()]
        );
        assert!(!candidates.contains(&gui));
        assert_eq!(
            windows_preferred_tool_executable("opencode", &gui),
            Some(sidecar)
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_skips_electron_desktop_without_cli_sidecar() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let gui = dir.path().join("opencode.exe");
        let resources = dir.path().join("resources");
        std::fs::create_dir(&resources).expect("resources dir should be created");
        std::fs::write(&gui, "").expect("desktop executable should be created");
        std::fs::write(resources.join("app.asar"), "").expect("app.asar should be created");

        let candidates = tool_executable_candidates("opencode", dir.path());

        assert_eq!(candidates, vec![dir.path().join("opencode.cmd")]);
        assert!(!candidates.contains(&gui));
        assert_eq!(windows_preferred_tool_executable("opencode", &gui), None);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_preferred_tool_executable_keeps_standalone_opencode_exe() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let standalone = dir.path().join("opencode.exe");
        std::fs::write(&standalone, "").expect("standalone executable should be created");

        assert_eq!(
            windows_preferred_tool_executable("opencode", &standalone),
            Some(standalone)
        );
    }

    #[test]
    fn foreign_pip_launcher_only_rejects_pi_exe_in_scripts_dir() {
        let pip_pi = Path::new(r"D:\soft\Python\Python311\Scripts\pi.exe");
        assert!(is_foreign_pip_launcher("pi", pip_pi));
        assert!(is_foreign_pip_launcher(
            "pi",
            Path::new(r"C:\venvs\tools\SCRIPTS\PI.EXE")
        ));
        assert!(!is_foreign_pip_launcher(
            "pi",
            Path::new(r"C:\Users\me\AppData\Roaming\npm\pi.cmd")
        ));
        // A custom npm prefix under a `Scripts` folder writes `.cmd` shims.
        assert!(!is_foreign_pip_launcher(
            "pi",
            Path::new(r"D:\Scripts\npm-global\pi.cmd")
        ));
        assert!(!is_foreign_pip_launcher(
            "pi",
            Path::new(r"D:\Scripts\pi.cmd")
        ));
        // Bun's global shims are `.exe` but live in `.bun\bin`.
        assert!(!is_foreign_pip_launcher(
            "pi",
            Path::new(r"C:\Users\me\.bun\bin\pi.exe")
        ));
        // Hermes really is installed by pip.
        assert!(!is_foreign_pip_launcher(
            "hermes",
            Path::new(r"D:\soft\Python\Python311\Scripts\hermes.exe")
        ));
    }

    #[test]
    fn vscode_copilot_shim_is_rejected_but_real_copilot_is_kept() {
        assert!(is_vscode_copilot_shim(
            "copilot",
            Path::new(r"D:\Microsoft VS Code\bin\copilot-shim\copilot.exe")
        ));
        assert!(is_vscode_copilot_shim(
            "copilot",
            Path::new(r"C:\Program Files\Microsoft VS Code\bin\COPILOT-SHIM\copilot.cmd")
        ));
        // npm's global shim is the real CLI.
        assert!(!is_vscode_copilot_shim(
            "copilot",
            Path::new(r"C:\Users\me\AppData\Roaming\npm\copilot.cmd")
        ));
        // Only the copilot tool is affected.
        assert!(!is_vscode_copilot_shim(
            "codex",
            Path::new(r"D:\x\copilot-shim\codex.exe")
        ));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_path_default_skips_pip_pi_and_uses_npm_pi() {
        let root = tempfile::tempdir().expect("temp dir should be created");
        let scripts = root.path().join("Python311").join("Scripts");
        std::fs::create_dir_all(&scripts).expect("Scripts dir should be created");
        let pip_pi = scripts.join("pi.exe");
        std::fs::write(&pip_pi, "").expect("pip launcher should be created");
        assert!(tool_executable_candidates("pi", &scripts)
            .iter()
            .all(|candidate| candidate != &pip_pi));

        let npm = root.path().join("npm");
        std::fs::create_dir_all(&npm).expect("npm dir should be created");
        let npm_pi = npm.join("pi.cmd");
        std::fs::write(&npm_pi, "").expect("npm shim should be created");

        let raw = format!("{}\r\n{}\r\n", pip_pi.display(), npm_pi.display());
        assert_eq!(windows_preferred_path_default("pi", &raw), Some(npm_pi));
        assert_eq!(
            windows_preferred_path_default("pi", &format!("{}\r\n", pip_pi.display())),
            None
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_path_default_skips_desktop_without_sidecar_and_uses_next_cli() {
        let desktop_dir = tempfile::tempdir().expect("desktop temp dir should be created");
        let resources = desktop_dir.path().join("resources");
        std::fs::create_dir(&resources).expect("resources dir should be created");
        let desktop = desktop_dir.path().join("opencode.exe");
        std::fs::write(&desktop, "").expect("desktop executable should be created");
        std::fs::write(resources.join("app.asar"), "").expect("app.asar should be created");

        let cli_dir = tempfile::tempdir().expect("CLI temp dir should be created");
        let cli = cli_dir.path().join("opencode.exe");
        std::fs::write(&cli, "").expect("standalone executable should be created");

        let raw = format!("{}\r\n{}\r\n", desktop.display(), cli.display());

        assert_eq!(windows_preferred_path_default("opencode", &raw), Some(cli));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn tool_executable_candidates_windows_skips_shadowed_npm_unix_shim() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let extensionless = dir.path().join("codex");
        let cmd = dir.path().join("codex.cmd");
        std::fs::write(&extensionless, "").expect("extensionless shim should be created");
        std::fs::write(&cmd, "").expect("cmd shim should be created");

        let candidates = tool_executable_candidates("codex", dir.path());

        assert_eq!(candidates, vec![cmd.clone(), dir.path().join("codex.exe")]);
        assert!(!candidates.contains(&extensionless));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_runnable_sibling_prefers_cmd_over_extensionless_tool() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let extensionless = dir.path().join("codex");
        let cmd = dir.path().join("codex.cmd");
        std::fs::write(&extensionless, "").expect("extensionless shim should be created");
        std::fs::write(&cmd, "").expect("cmd shim should be created");

        let preferred = windows_runnable_sibling_for_extensionless_tool(&extensionless);

        assert_eq!(preferred.as_deref(), Some(cmd.as_path()));
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_tool_capture_accepts_canonicalized_batch_paths() {
        let root = tempfile::tempdir().unwrap();
        let bin = root.path().join("模型 tools & helpers");
        std::fs::create_dir(&bin).unwrap();
        std::fs::write(
            bin.join("cc_switch_capture_helper.cmd"),
            "@echo helper-found\r\n@exit /b 23\r\n",
        )
        .unwrap();
        let path = format!("{};{}", bin.display(), effective_path_string());
        for extension in ["cmd", "bat"] {
            let entry = bin.join(format!("opencode.{extension}"));
            std::fs::write(
                &entry,
                "@echo off\r\necho arg=%~1\r\necho env=%CC_SWITCH_CAPTURE_TEST%\r\necho captured-stderr 1>&2\r\ncmd /D /C cc_switch_capture_helper.cmd\r\nexit /b %errorlevel%\r\n",
            )
            .unwrap();
            let canonical = std::fs::canonicalize(entry).unwrap();
            assert!(canonical.to_string_lossy().starts_with(r"\\?\"));
            let output = run_windows_tool_command_capture(
                &canonical,
                &["models"],
                &path,
                CommandDeadline::from_timeout(Some(std::time::Duration::from_secs(10))),
                &[("CC_SWITCH_CAPTURE_TEST", "preserved".into())],
                root.path(),
            )
            .unwrap();
            let stdout = decode_command_output(&output.stdout);
            assert_eq!(output.status.code(), Some(23), "{output:?}");
            assert!(stdout.contains("arg=models"), "{stdout}");
            assert!(stdout.contains("env=preserved"), "{stdout}");
            assert!(stdout.contains("helper-found"), "{stdout}");
            assert!(decode_command_output(&output.stderr).contains("captured-stderr"));
        }
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn windows_shell_compatible_path_strips_verbatim_prefixes() {
        assert_eq!(
            windows_shell_compatible_path(Path::new(r"\\?\C:\tools\codex.cmd")),
            PathBuf::from(r"C:\tools\codex.cmd")
        );
        assert_eq!(
            windows_shell_compatible_path(Path::new(r"\\?\UNC\server\share\tools\codex.cmd")),
            PathBuf::from(r"\\server\share\tools\codex.cmd")
        );
        assert_eq!(
            windows_shell_compatible_path(Path::new(r"C:\tools\codex.cmd")),
            PathBuf::from(r"C:\tools\codex.cmd")
        );
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn run_windows_tool_version_command_accepts_canonicalized_cmd_path() {
        let dir = tempfile::tempdir().expect("temp dir should be created");
        let cmd = dir.path().join("codex.cmd");
        std::fs::write(&cmd, "@echo off\r\necho codex-cli 0.144.3\r\n")
            .expect("cmd shim should be created");
        let canonical = std::fs::canonicalize(&cmd).expect("cmd shim should canonicalize");
        assert!(
            canonical.to_string_lossy().starts_with(r"\\?\"),
            "Windows canonical paths should use the verbatim prefix: {}",
            canonical.display()
        );

        let current_path = std::env::var("PATH").unwrap_or_default();
        let output = run_windows_tool_version_command(&canonical, &current_path)
            .expect("canonicalized cmd shim should execute");
        let stderr = decode_command_output(&output.stderr);

        assert!(output.status.success(), "cmd shim failed: {stderr}");
        assert_eq!(
            decode_command_output(&output.stdout).trim(),
            "codex-cli 0.144.3"
        );
    }

    #[test]
    fn resolve_launch_cwd_accepts_existing_directory() {
        let resolved =
            resolve_launch_cwd(Some(std::env::temp_dir().to_string_lossy().into_owned()))
                .expect("temp dir should resolve")
                .expect("temp dir should be present");

        assert!(resolved.is_dir());
    }

    #[test]
    fn resolve_launch_cwd_rejects_missing_directory() {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .expect("clock should be after epoch")
            .as_nanos();
        let missing = std::env::temp_dir().join(format!("cc-switch-missing-{unique}"));

        let error = resolve_launch_cwd(Some(missing.to_string_lossy().into_owned()))
            .expect_err("missing directory should fail");

        assert!(error.contains("目录不存在"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn iterm2_applescript_cold_start_avoids_current_window_before_one_exists() {
        let script = build_macos_iterm2_applescript(Path::new("/tmp/cc_switch_launcher.sh"));

        let cold_start_branch = script
            .split("else\n        activate")
            .nth(1)
            .expect("cold start branch should be present")
            .split("    end if\n    tell current session")
            .next()
            .expect("cold start branch should end before writing command");

        assert!(cold_start_branch.contains("repeat while (count of windows) = 0"));
        assert!(cold_start_branch.contains("create window with default profile"));
        assert!(!cold_start_branch.contains("tell current window"));
        assert!(!cold_start_branch.contains("create tab with default profile"));
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn iterm2_applescript_keeps_new_tab_behavior_for_existing_windows() {
        let script = build_macos_iterm2_applescript(Path::new("/tmp/cc_switch_launcher.sh"));

        let running_branch = script
            .split("if was_running then")
            .nth(1)
            .expect("already-running branch should be present")
            .split("else\n        activate")
            .next()
            .expect("already-running branch should end before cold start branch");

        assert!(running_branch.contains("if (count of windows) = 0 then"));
        assert!(running_branch.contains("create window with default profile"));
        assert!(running_branch.contains("create tab with default profile"));
    }

    /// Terminal `activate` creates a default empty window on cold start; `launch` does not.
    #[cfg(target_os = "macos")]
    #[test]
    fn terminal_applescript_cold_start_uses_launch_before_do_script() {
        let script = build_macos_terminal_applescript(Path::new("/tmp/cc_switch_launcher.sh"));

        assert!(
            script.contains(r#"set was_running to application "Terminal" is running"#),
            "missing was_running detection:\n{script}"
        );
        // Cold launches avoid `activate` until after `do script`, so no default empty window is created first.
        assert!(
            script.contains(
                "else\n        launch\n        do script launcher_script\n        activate"
            ),
            "cold start should launch before activating:\n{script}"
        );
        // Already-running launches should create a fresh session.
        assert!(
            script.contains(
                "if was_running then\n        activate\n        do script launcher_script\n"
            ),
            "already-running branch should use bare do script:\n{script}"
        );
        assert!(
            script.contains(r#"set launcher_script to "exec sh '/tmp/cc_switch_launcher.sh'""#),
            "Terminal should replace the auto-created shell:\n{script}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn otty_launcher_command_executes_the_temporary_script() {
        assert_eq!(
            build_macos_dash_c_command(Path::new("/tmp/cc_switch_launcher.sh")),
            "exec sh '/tmp/cc_switch_launcher.sh'"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn otty_cli_candidates_include_bundle_and_installed_cli_locations() {
        let candidates = macos_otty_cli_candidates();

        assert!(candidates.contains(&PathBuf::from(
            "/Applications/Otty.app/Contents/MacOS/otty-cli"
        )));
        assert!(candidates.contains(&PathBuf::from("/usr/local/bin/otty")));
        assert!(candidates.contains(&PathBuf::from("/opt/homebrew/bin/otty")));
    }

    /// Restored windows should not receive the launcher command.
    #[cfg(target_os = "macos")]
    #[test]
    fn terminal_applescript_does_not_hijack_restored_windows() {
        let script = build_macos_terminal_applescript(Path::new("/tmp/cc_switch_launcher.sh"));
        assert!(
            !script.contains(" in window 1"),
            "should not inject into an existing/restored Terminal window:\n{script}"
        );
        assert!(
            !script.contains("count of windows"),
            "should not infer restored-window safety from window count:\n{script}"
        );
    }

    /// Ghostty cold starts use `initial-command`; warm starts use the scripting dictionary.
    #[cfg(target_os = "macos")]
    #[test]
    fn ghostty_applescript_cold_start_uses_initial_command() {
        let script = build_macos_ghostty_applescript(Path::new("/tmp/cc_switch_launcher.sh"));

        // Warm launches execute through the AppleScript command property, not `open -na ... -e`.
        assert!(
            script.contains(r#"set launcher_command to "sh '/tmp/cc_switch_launcher.sh'""#),
            "missing launcher_command:\n{script}"
        );
        assert!(script.contains("if was_running then"));
        assert!(script.contains("new window with configuration {command:launcher_command}"));
        assert!(
            !script.contains(" --args -e"),
            "should not execute through open -na -e:\n{script}"
        );
        // Cold launches make Ghostty's first default surface execute the launcher.
        assert!(script.contains(r#"set was_running to application "Ghostty" is running"#));
        assert!(
            script.contains(
                r#"do shell script "open -na Ghostty --args --quit-after-last-window-closed=true " & quoted form of ("--initial-command=" & launcher_command)"#
            ),
            "cold start should use initial-command:\n{script}"
        );
        assert!(
            !script.contains("--initial-window=false"),
            "should not rely on initial-window=false:\n{script}"
        );
        assert!(
            !script.contains("delay 0.5"),
            "should not rely on a fixed delay:\n{script}"
        );
        assert!(
            !script.contains("old_ids"),
            "should not track default windows for closing:\n{script}"
        );
        assert!(
            !script.contains("close window"),
            "should not close a default window:\n{script}"
        );
    }

    #[cfg(target_os = "macos")]
    #[test]
    fn dash_c_command_wraps_script_path_inside_quoted_arg() {
        // The script path must stay inside the `-c` string, not as a bare argv.
        let s = build_macos_dash_c_command(Path::new("/tmp/cc_switch_launcher_1.sh"));
        assert_eq!(s, "exec sh '/tmp/cc_switch_launcher_1.sh'");

        // Spaces and single quotes must stay shell-safe too.
        let s2 = build_macos_dash_c_command(Path::new("/Users/me/it's dir/x.sh"));
        assert_eq!(s2, r#"exec sh '/Users/me/it'"'"'s dir/x.sh'"#);
    }

    /// AppleScript launchers need both shell-path quoting and AppleScript string quoting.
    #[cfg(target_os = "macos")]
    #[test]
    fn applescript_builders_safely_quote_special_paths() {
        // First shell-quote the path, then wrap the whole command as an AppleScript string.
        let expected = r#""sh '/Users/me/it'\"'\"'s dir/x.sh'""#;
        let p = Path::new("/Users/me/it's dir/x.sh");
        assert_eq!(applescript_launcher_command(p), expected);
        assert_eq!(
            applescript_exec_launcher_command(p),
            r#""exec sh '/Users/me/it'\"'\"'s dir/x.sh'""#
        );
        assert!(
            build_macos_terminal_applescript(p)
                .contains(r#""exec sh '/Users/me/it'\"'\"'s dir/x.sh'""#),
            "Terminal did not quote safely"
        );
        assert!(
            build_macos_iterm2_applescript(p)
                .contains(r#""exec sh '/Users/me/it'\"'\"'s dir/x.sh'""#),
            "iTerm2 did not quote safely"
        );
        assert!(
            build_macos_ghostty_applescript(p).contains(expected),
            "Ghostty did not keep the non-exec launcher"
        );
    }

    #[test]
    fn build_windows_cwd_command_str_uses_cd_for_drive_paths() {
        let (command, env) = build_windows_cwd_command_str(r"C:\work\repo");

        assert_eq!(command, "cd /d \"C:\\work\\repo\" || exit /b 1\r\n");
        assert_eq!(env, None);
    }

    #[test]
    fn build_windows_cwd_command_str_uses_pushd_for_unc_paths() {
        let (command, _) = build_windows_cwd_command_str(r"\\wsl$\Ubuntu\home\coder\repo");

        assert_eq!(
            command,
            "pushd \"\\\\wsl$\\Ubuntu\\home\\coder\\repo\" || exit /b 1\r\n"
        );
    }

    #[test]
    fn build_windows_cwd_command_str_escapes_batch_metacharacters() {
        let (command, _) = build_windows_cwd_command_str(r"\\server\share\100%&(test)");

        assert_eq!(
            command,
            "pushd \"\\\\server\\share\\100%%^&^(test^)\" || exit /b 1\r\n"
        );
    }

    #[test]
    fn build_windows_cwd_command_str_passes_non_ascii_paths_via_env() {
        // cmd 按控制台代码页解码 .bat，中文路径不能写进文件（#8034）。
        let path = r"C:\Users\林增文\项目";
        let (command, env) = build_windows_cwd_command_str(path);

        assert!(command.is_ascii(), "batch line must stay ASCII: {command}");
        assert_eq!(
            command,
            "cd /d \"%CC_SWITCH_INTERNAL_CWD%\" || exit /b 1\r\nset \"CC_SWITCH_INTERNAL_CWD=\"\r\n"
        );
        assert_eq!(env.as_deref(), Some(path));

        let (unc_command, unc_env) = build_windows_cwd_command_str(r"\\server\共享\repo");
        assert!(unc_command.starts_with("pushd \"%CC_SWITCH_INTERNAL_CWD%\" || exit /b 1\r\n"));
        assert_eq!(unc_env.as_deref(), Some(r"\\server\共享\repo"));
    }
}
