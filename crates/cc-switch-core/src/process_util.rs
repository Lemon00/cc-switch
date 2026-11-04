//! 运行外部工具命令的 helper。
//!
//! 来源：GUI `commands/misc.rs` 里的同名函数。P1 先放一份核心版，
//! 行为差异（见下）留待 P3 由 GUI 命令层统一调用核心版时消除：
//! - 工具定位只查 PATH，不做 GUI 那套多安装源枚举 / mise shim 解析；
//! - Windows 下不走 WSL 分发，直接跑本机命令。

use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::time::Duration;

const VALID_TOOLS: [&str; 13] = [
    "claude", "codex", "gemini", "grok", "opencode", "openclaw", "hermes", "pi", "mcode",
    "kimi", "dsh", "agy", "omp",
];

pub fn decode_command_output(bytes: &[u8]) -> String {
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
    if let Some(decoded) = decode_codepage(bytes, ansi_cp) {
        return decoded;
    }
    String::from_utf8_lossy(bytes).into_owned()
}

/// 本机工具版本（GUI `commands::local_tool_version` 的核心版简化版）：
/// 只跑 `tool --version`，取输出里第一个像版本号的 token。
/// 不做 GUI 版的登录 shell / WSL 探测。
pub fn local_tool_version(tool: &str) -> Option<String> {
    if !VALID_TOOLS.contains(&tool) {
        return None;
    }
    let out = run_detected_tool_command_with_timeout(
        tool,
        &["--version"],
        Some(Duration::from_secs(10)),
        &[],
        std::env::current_dir().as_deref().unwrap_or(Path::new(".")),
    )
    .ok()?;
    if !out.status.success() {
        return None;
    }
    let text = decode_command_output(&out.stdout);
    text.split_whitespace().find_map(|token| {
        let token = token.trim_start_matches('v');
        let mut parts = token.split('.');
        let major = parts.next()?;
        if !major.chars().all(|c| c.is_ascii_digit()) || major.is_empty() {
            return None;
        }
        let minor = parts.next()?;
        if !minor.chars().all(|c| c.is_ascii_digit()) || minor.is_empty() {
            return None;
        }
        Some(token.trim_end_matches(|c: char| !c.is_ascii_alphanumeric() && c != '.').to_string())
    })
}

/// 构建一条跑工具的 Command（GUI `commands::detected_tool_command` 的核心版简化版）：
/// 只查 PATH，不做登录 shell 的 PATH 合并与多安装源枚举；
/// 非 Windows 下子进程 setsid 脱离控制终端（与 GUI 版一致）。
pub fn detected_tool_command(
    tool: &str,
    args: &[&str],
    _timeout: Duration,
) -> Result<std::process::Command, String> {
    if !VALID_TOOLS.contains(&tool) {
        return Err(format!("Unsupported tool: {tool}"));
    }
    let tool_path = find_in_path(tool).ok_or_else(|| format!("{tool} is not installed"))?;
    let dir = tool_path
        .parent()
        .ok_or_else(|| format!("Invalid {tool} executable path"))?;
    let mut cmd = std::process::Command::new(&tool_path);
    cmd.args(args);
    #[cfg(not(target_os = "windows"))]
    {
        let inherited = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        cmd.env("PATH", format!("{}:{inherited}", dir.display()));
        use std::os::unix::process::CommandExt;
        unsafe {
            cmd.pre_exec(|| {
                if libc::setsid() == -1 {
                    return Err(std::io::Error::last_os_error());
                }
                Ok(())
            });
        }
    }
    Ok(cmd)
}

/// 工具是否跑在 WSL 里（核心版：非 Windows 恒为 false；Windows 下暂不做
/// 注册表探测，返回 false——GUI 侧仍用 misc 版）。
pub fn tool_runs_in_wsl(tool: &str) -> bool {
    #[cfg(target_os = "windows")]
    {
        let _ = tool;
        false
    }
    #[cfg(not(target_os = "windows"))]
    {
        let _ = tool;
        false
    }
}

/// 杀掉子进程整棵树：非 Windows 下 kill(-pid) 整组击杀（子进程须由
/// detected_tool_command 建出、已 setsid）；Windows 下 taskkill /T /F。
pub fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    #[cfg(target_os = "windows")]
    {
        use std::process::{Command, Stdio};
        let status = Command::new("taskkill")
            .args(["/PID", &child.id().to_string(), "/T", "/F"])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        matches!(status, Ok(status) if status.success()) || child.kill().is_ok()
    }
    #[cfg(not(target_os = "windows"))]
    {
        let process_group = -(child.id() as libc::pid_t);
        // SAFETY: runtime commands are placed in a dedicated process group before spawn.
        (unsafe { libc::kill(process_group, libc::SIGKILL) == 0 }) || child.kill().is_ok()
    }
}

fn find_in_path(tool: &str) -> Option<PathBuf> {
    let path_var = std::env::var_os("PATH")?;
    for dir in std::env::split_paths(&path_var) {
        let candidate = dir.join(tool);
        if candidate.is_file() {
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                if let Ok(meta) = std::fs::metadata(&candidate) {
                    if meta.permissions().mode() & 0o111 != 0 {
                        return Some(candidate);
                    }
                    continue;
                }
            }
            #[cfg(not(unix))]
            {
                return Some(candidate);
            }
        }
    }
    None
}

/// 在超时内跑工具命令；超时则杀掉子进程并返回超时错误。
/// 参数校验与 GUI 版一致：工具必须在白名单里，参数只能是 ASCII 字母数字
/// 与 `-_.`。
pub fn run_detected_tool_command_with_timeout(
    tool: &str,
    args: &[&str],
    timeout: Option<Duration>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<Output, String> {
    if !VALID_TOOLS.contains(&tool) {
        return Err(format!("Unsupported tool: {tool}"));
    }
    if args.iter().any(|arg| {
        arg.is_empty()
            || !arg
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_' | '.'))
    }) {
        return Err("Invalid tool command arguments".to_string());
    }
    let tool_path = find_in_path(tool).ok_or_else(|| format!("{tool} is not installed"))?;
    let mut cmd = Command::new(tool_path);
    cmd.args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .current_dir(working_dir);
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let mut child = cmd.spawn().map_err(|e| format!("启动 {tool} 失败: {e}"))?;
    // 轮询 try_wait：超时时能主动 kill，退出后 wait_with_output 收输出。
    let start = std::time::Instant::now();
    loop {
        match child
            .try_wait()
            .map_err(|e| format!("等待 {tool} 退出失败: {e}"))?
        {
            Some(_) => {
                return child
                    .wait_with_output()
                    .map_err(|e| format!("等待 {tool} 退出失败: {e}"));
            }
            None => {
                if let Some(limit) = timeout {
                    if start.elapsed() >= limit {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(format!("{tool} 执行超时"));
                    }
                }
                std::thread::sleep(Duration::from_millis(20));
            }
        }
    }
}
