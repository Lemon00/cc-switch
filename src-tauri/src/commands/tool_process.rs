use super::*;

#[cfg(not(windows))]
pub(super) use cc_switch_core::process_output::isolate_child_process_group;
pub(crate) use cc_switch_core::process_output::{
    output_with_timeout, terminate_child_tree, wait_child_output, CommandDeadline,
};

fn apply_extra_env(cmd: &mut std::process::Command, extra_env: &[(&str, String)]) {
    for (key, value) in extra_env {
        cmd.env(key, value);
    }
}

pub(crate) fn run_detected_tool_command_with_timeout(
    tool: &str,
    args: &[&str],
    timeout: Option<std::time::Duration>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<std::process::Output, String> {
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

    let deadline = CommandDeadline::from_timeout(timeout);

    #[cfg(target_os = "windows")]
    if let Some(distro) = wsl_distro_for_tool(tool) {
        return run_wsl_tool_command(tool, args, &distro, deadline, extra_env, working_dir);
    }

    // Runtime execution only needs the default entry point. Full installation
    // enumeration runs `--version` for every candidate and belongs to diagnostics.
    let tool_path = locate_default_tool(tool, deadline)?;
    let dir = tool_path
        .parent()
        .ok_or_else(|| format!("Invalid {tool} executable path"))?;
    #[cfg(target_os = "windows")]
    let current_path = effective_path_string();
    // GUI 启动时继承的是 launchd 的窄 PATH（/usr/bin:/bin:/usr/sbin:/sbin）。npm 装的 CLI
    // 是 `#!/usr/bin/env node` 脚本，node 常不在工具自己的目录里（如 codex 在
    // /opt/homebrew/bin、node 在 /usr/local/bin），只补工具目录会 exit 127（#8653）。
    // 与安装/升级同样补上登录 shell 的 PATH。
    #[cfg(not(target_os = "windows"))]
    let current_path = {
        let inherited = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        match login_shell_path(deadline) {
            Some(login_path) => merge_path_segments(&login_path, &inherited),
            None => inherited,
        }
    };

    #[cfg(target_os = "windows")]
    {
        run_windows_tool_command_capture(
            &tool_path,
            args,
            &format!("{};{current_path}", dir.display()),
            deadline,
            extra_env,
            working_dir,
        )
    }

    #[cfg(not(target_os = "windows"))]
    {
        use std::process::{Command, Stdio};

        let mut cmd = Command::new(&tool_path);
        cmd.args(args)
            .env("PATH", format!("{}:{current_path}", dir.display()))
            .current_dir(working_dir)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        apply_extra_env(&mut cmd, extra_env);
        isolate_child_process_group(&mut cmd);
        let child = cmd
            .spawn()
            .map_err(|e| format!("Failed to run {tool}: {e}"))?;
        wait_child_output(child, deadline)
    }
}

/// 构造运行本机工具默认入口的命令（定位规则和 PATH 补全同 [`run_detected_tool_command_with_timeout`]），
/// 不设 stdio、不等待：给要自己管子进程的调用方用（比如长时间运行、要往 stdin 写东西的登录）。
/// `timeout` 只约束定位工具这一步。工具配置在 WSL 里时报错。
pub(crate) fn detected_tool_command(
    tool: &str,
    args: &[&str],
    timeout: std::time::Duration,
) -> Result<std::process::Command, String> {
    if !VALID_TOOLS.contains(&tool) {
        return Err(format!("Unsupported tool: {tool}"));
    }
    let deadline = CommandDeadline::from_timeout(Some(timeout));

    #[cfg(target_os = "windows")]
    if wsl_distro_for_tool(tool).is_some() {
        return Err(format!(
            "{tool} runs in WSL; this action needs it on Windows"
        ));
    }

    let tool_path = locate_default_tool(tool, deadline)?;
    let dir = tool_path
        .parent()
        .ok_or_else(|| format!("Invalid {tool} executable path"))?;

    #[cfg(target_os = "windows")]
    {
        let new_path = format!("{};{}", dir.display(), effective_path_string());
        Ok(build_windows_tool_command(&tool_path, args, &new_path))
    }

    #[cfg(not(target_os = "windows"))]
    {
        let inherited = std::env::var_os("PATH")
            .map(|value| value.to_string_lossy().into_owned())
            .unwrap_or_default();
        let current_path = match login_shell_path(deadline) {
            Some(login_path) => merge_path_segments(&login_path, &inherited),
            None => inherited,
        };
        let mut cmd = std::process::Command::new(&tool_path);
        cmd.args(args)
            .env("PATH", format!("{}:{current_path}", dir.display()));
        isolate_child_process_group(&mut cmd);
        Ok(cmd)
    }
}

#[cfg(target_os = "windows")]
pub(super) fn run_windows_tool_command_capture(
    tool_path: &Path,
    args: &[&str],
    new_path: &str,
    deadline: Option<CommandDeadline>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;

    let mut cmd = build_windows_tool_command(tool_path, args, new_path);
    apply_extra_env(&mut cmd, extra_env);
    cmd.current_dir(working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("Failed to run tool: {e}"))?;
    wait_child_output(child, deadline)
}

/// Convert `\\wsl$\Distro\home\user\...` / `\\wsl.localhost\...` to a Linux path.
#[cfg(target_os = "windows")]
fn wsl_unc_path_to_linux(path: &Path) -> Option<String> {
    use std::path::{Component, Prefix};

    let mut components = path.components();
    let Component::Prefix(prefix) = components.next()? else {
        return None;
    };
    match prefix.kind() {
        Prefix::UNC(server, _share) | Prefix::VerbatimUNC(server, _share) => {
            let server_name = server.to_string_lossy();
            if !(server_name.eq_ignore_ascii_case("wsl$")
                || server_name.eq_ignore_ascii_case("wsl.localhost"))
            {
                return None;
            }
        }
        _ => return None,
    }

    let mut linux = String::new();
    for component in components {
        match component {
            Component::RootDir => {}
            Component::Normal(part) => {
                linux.push('/');
                linux.push_str(&part.to_string_lossy());
            }
            Component::CurDir | Component::ParentDir | Component::Prefix(_) => return None,
        }
    }
    if linux.is_empty() {
        None
    } else {
        Some(linux)
    }
}

#[cfg(target_os = "windows")]
pub(super) fn build_wsl_env_argv(extra_env: &[(&str, String)]) -> Result<Vec<String>, String> {
    let mut env_argv = Vec::new();
    for (key, value) in extra_env {
        if key.is_empty()
            || key.contains('=')
            || key.chars().any(|c| c.is_whitespace() || c.is_control())
        {
            return Err(format!("invalid env for {key}"));
        }

        let linux_value = if *key == "OPENCODE_CONFIG_DIR" {
            let Some(value) = wsl_unc_path_to_linux(Path::new(value)) else {
                continue;
            };
            value
        } else {
            value.clone()
        };
        if linux_value.chars().any(char::is_control) {
            return Err(format!("invalid env for {key}"));
        }
        env_argv.push(format!("{key}={linux_value}"));
    }
    Ok(env_argv)
}

#[cfg(target_os = "windows")]
fn build_wsl_tool_command(
    tool: &str,
    args: &[&str],
    deadline: Option<CommandDeadline>,
) -> Result<String, String> {
    let invocation = std::iter::once(tool)
        .chain(args.iter().copied())
        .collect::<Vec<_>>()
        .join(" ");
    let command = format!(
        "for flag in -lic -lc -c; do if \"${{SHELL:-sh}}\" \"$flag\" 'command -v {tool}' >/dev/null 2>&1; then exec \"${{SHELL:-sh}}\" \"$flag\" '{invocation}'; fi; done; exit 127"
    );

    let Some(deadline) = deadline else {
        return Ok(command);
    };
    let remaining = deadline.remaining()?;
    let timeout_arg = format!("{:.3}s", remaining.as_secs_f64());
    Ok(format!(
        "command -v timeout >/dev/null 2>&1 || {{ echo 'timeout is required for bounded CLI execution' >&2; exit 127; }}; exec timeout --signal=TERM --kill-after=1s {timeout_arg} sh -c {}",
        shell_single_quote(&command)
    ))
}

#[cfg(target_os = "windows")]
fn run_wsl_tool_command(
    tool: &str,
    args: &[&str],
    distro: &str,
    deadline: Option<CommandDeadline>,
    extra_env: &[(&str, String)],
    working_dir: &Path,
) -> Result<std::process::Output, String> {
    use std::process::{Command, Stdio};

    if !is_valid_wsl_distro_name(distro) {
        return Err(format!("[WSL:{distro}] invalid distro name"));
    }

    let command = build_wsl_tool_command(tool, args, deadline)?;
    let linux_working_dir = wsl_unc_path_to_linux(working_dir)
        .ok_or_else(|| format!("[WSL:{distro}] invalid working directory"))?;
    let env_argv = build_wsl_env_argv(extra_env).map_err(|e| format!("[WSL:{distro}] {e}"))?;

    let mut cmd = Command::new("wsl.exe");
    cmd.arg("-d")
        .arg(distro)
        .arg("--cd")
        .arg(linux_working_dir)
        .arg("--");
    if !env_argv.is_empty() {
        cmd.arg("env");
        for item in &env_argv {
            cmd.arg(item);
        }
    }
    cmd.args(["sh", "-c", &command])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    let child = cmd
        .spawn()
        .map_err(|e| format!("[WSL:{distro}] failed to run {tool}: {e}"))?;
    let output = wait_child_output(child, deadline).map_err(|e| {
        if e.starts_with("Command timed out") {
            format!("[WSL:{distro}] {e}")
        } else {
            e
        }
    })?;
    if output.status.code() == Some(124) {
        return Err(format!(
            "[WSL:{distro}] {}",
            deadline
                .map(CommandDeadline::timeout_error)
                .unwrap_or_else(|| "Command timed out".to_string())
        ));
    }
    Ok(output)
}
