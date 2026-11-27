//! Shared bounded capture for finite CLI commands. Unix callers must isolate the process group.
#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x08000000;

#[derive(Clone, Copy)]
pub struct CommandDeadline {
    expires_at: std::time::Instant,
    limit: std::time::Duration,
}

impl CommandDeadline {
    pub fn from_timeout(timeout: Option<std::time::Duration>) -> Option<Self> {
        timeout.map(|limit| Self {
            expires_at: std::time::Instant::now() + limit,
            limit,
        })
    }

    pub fn remaining(self) -> Result<std::time::Duration, String> {
        self.expires_at
            .checked_duration_since(std::time::Instant::now())
            .filter(|remaining| !remaining.is_zero())
            .ok_or_else(|| self.timeout_error())
    }

    pub fn timeout_error(self) -> String {
        format!("Command timed out after {}s", self.limit.as_secs())
    }
}

#[cfg(target_os = "windows")]
pub fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};

    let status = Command::new("taskkill")
        .args(["/PID", &child.id().to_string(), "/T", "/F"])
        .creation_flags(CREATE_NO_WINDOW)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status();
    matches!(status, Ok(status) if status.success()) || child.kill().is_ok()
}

#[cfg(not(target_os = "windows"))]
pub fn terminate_child_tree(child: &mut std::process::Child) -> bool {
    let process_group = -(child.id() as libc::pid_t);
    // SAFETY: runtime commands are placed in a dedicated process group before spawn.
    (unsafe { libc::kill(process_group, libc::SIGKILL) == 0 }) || child.kill().is_ok()
}

#[cfg(not(target_os = "windows"))]
pub fn isolate_child_process_group(cmd: &mut std::process::Command) {
    use std::os::unix::process::CommandExt;

    // setsid 而非 process_group(0)：新会话自带新进程组（组长=自身，
    // terminate_child_tree 的 kill(-pid) 整组击杀语义不变），并额外**脱离控制终端**。
    // 只隔离进程组时，探测用的交互式 shell（zsh -lic）若还持有控制终端（如 dev 模式
    // 从终端启动），其作业控制会因处于背景进程组被 SIGTTIN/SIGTTOU 停住，`wait()`
    // 永远等不到退出；脱离终端后 shell 拿不到 /dev/tty，作业控制自动关闭。
    // SAFETY: setsid 是 async-signal-safe；fork 出的子进程继承父进程组、必不是组长，
    // 调用不会因 EPERM 失败。
    unsafe {
        cmd.pre_exec(|| {
            if libc::setsid() == -1 {
                return Err(std::io::Error::last_os_error());
            }
            Ok(())
        });
    }
}

const MAX_COMMAND_OUTPUT_BYTES: usize = 4 * 1024 * 1024;

/// Capture a finite command without inheriting stdin or blocking on a full pipe.
pub fn output_with_timeout(
    command: &mut std::process::Command,
    timeout: std::time::Duration,
) -> Result<std::process::Output, String> {
    use std::process::Stdio;
    let deadline = CommandDeadline::from_timeout(Some(timeout));
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    #[cfg(unix)]
    isolate_child_process_group(command);
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(CREATE_NO_WINDOW);
    }
    let child = command
        .spawn()
        .map_err(|error| format!("Failed to start command: {error}"))?;
    wait_child_output(child, deadline)
}

fn read_command_output(mut reader: impl std::io::Read) -> Result<Vec<u8>, String> {
    let mut output = Vec::new();
    let mut buffer = [0u8; 8192];
    let mut exceeded = false;
    loop {
        let count = match reader.read(&mut buffer) {
            Ok(count) => count,
            Err(error) if error.kind() == std::io::ErrorKind::Interrupted => continue,
            Err(error) => return Err(format!("Failed to read command output: {error}")),
        };
        if count == 0 {
            break;
        }
        if !exceeded && output.len().saturating_add(count) <= MAX_COMMAND_OUTPUT_BYTES {
            output.extend_from_slice(&buffer[..count]);
        } else {
            exceeded = true;
        }
    }
    if exceeded {
        Err(format!(
            "Command output exceeds {MAX_COMMAND_OUTPUT_BYTES} bytes"
        ))
    } else {
        Ok(output)
    }
}

pub fn wait_child_output(
    mut child: std::process::Child,
    deadline: Option<CommandDeadline>,
) -> Result<std::process::Output, String> {
    // A job still owns descendants after the original Windows process exits.
    // taskkill /PID alone cannot find those descendants once the parent is gone.
    #[cfg(windows)]
    let job = ChildJob::attach(&child);
    let terminate = |child: &mut std::process::Child| {
        #[cfg(windows)]
        if job.as_ref().is_some_and(ChildJob::terminate) {
            return true;
        }
        terminate_child_tree(child)
    };
    // Close captured stdin before polling: the child may be waiting for EOF.
    drop(child.stdin.take());
    let stdout_pipe = child.stdout.take();
    let stderr_pipe = child.stderr.take();

    let stdout_handle =
        stdout_pipe.map(|pipe| std::thread::spawn(move || read_command_output(pipe)));
    let stderr_handle =
        stderr_pipe.map(|pipe| std::thread::spawn(move || read_command_output(pipe)));

    let status = match deadline {
        None => match child.wait() {
            Ok(status) => status,
            Err(error) => {
                if terminate(&mut child) {
                    let _ = child.wait();
                }
                return Err(format!("Failed to wait for command: {error}"));
            }
        },
        Some(deadline) => {
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        let remaining = match deadline.remaining() {
                            Ok(remaining) => remaining,
                            Err(error) => {
                                if terminate(&mut child) {
                                    let _ = child.wait();
                                }
                                // Do not join pipe readers on timeout. If tree termination fails,
                                // a descendant may still own the write handle and never produce EOF.
                                drop(stdout_handle);
                                drop(stderr_handle);
                                return Err(error);
                            }
                        };
                        std::thread::sleep(std::cmp::min(
                            std::time::Duration::from_millis(50),
                            remaining,
                        ));
                    }
                    Err(e) => {
                        if terminate(&mut child) {
                            let _ = child.wait();
                        }
                        return Err(format!("Failed to wait for command: {e}"));
                    }
                }
            }
        }
    };

    if let Some(deadline) = deadline {
        while stdout_handle
            .as_ref()
            .is_some_and(|handle| !handle.is_finished())
            || stderr_handle
                .as_ref()
                .is_some_and(|handle| !handle.is_finished())
        {
            let remaining = match deadline.remaining() {
                Ok(remaining) => remaining,
                Err(error) => {
                    let _ = terminate(&mut child);
                    drop(stdout_handle);
                    drop(stderr_handle);
                    return Err(error);
                }
            };
            std::thread::sleep(std::cmp::min(
                std::time::Duration::from_millis(50),
                remaining,
            ));
        }
    }

    let stdout = stdout_handle
        .map(|handle| {
            handle
                .join()
                .map_err(|_| "Command stdout reader panicked".to_string())?
        })
        .transpose()?
        .unwrap_or_default();
    let stderr = stderr_handle
        .map(|handle| {
            handle
                .join()
                .map_err(|_| "Command stderr reader panicked".to_string())?
        })
        .transpose()?
        .unwrap_or_default();

    Ok(std::process::Output {
        status,
        stdout,
        stderr,
    })
}

#[cfg(windows)]
struct ChildJob(windows_sys::Win32::Foundation::HANDLE);

#[cfg(windows)]
impl ChildJob {
    fn attach(child: &std::process::Child) -> Option<Self> {
        use std::os::windows::io::AsRawHandle;
        use windows_sys::Win32::System::JobObjects::{AssignProcessToJobObject, CreateJobObjectW};
        // SAFETY: no security attributes/name; the child owns a valid process handle.
        unsafe {
            let handle = CreateJobObjectW(std::ptr::null(), std::ptr::null());
            if handle.is_null() {
                return None;
            }
            let job = Self(handle);
            if AssignProcessToJobObject(handle, child.as_raw_handle()) == 0 {
                return None;
            }
            Some(job)
        }
    }

    fn terminate(&self) -> bool {
        // SAFETY: this job handle stays owned until Drop.
        unsafe { windows_sys::Win32::System::JobObjects::TerminateJobObject(self.0, 1) != 0 }
    }
}

#[cfg(windows)]
impl Drop for ChildJob {
    fn drop(&mut self) {
        // Do not kill on successful completion: an installer may have started a service.
        unsafe {
            windows_sys::Win32::Foundation::CloseHandle(self.0);
        }
    }
}

#[cfg(test)]
mod bounded_output_tests {
    use super::*;
    use std::io::{Read, Write};
    use std::process::{Command, Stdio};
    use std::time::Duration;

    // Reuse the test binary as a portable child; no installed CLI or user config is touched.
    #[test]
    fn subprocess_fixture() {
        match std::env::var("CC_SWITCH_PROCESS_FIXTURE").as_deref() {
            Ok("output") => {
                std::io::stdout()
                    .write_all(&vec![b'o'; 1024 * 1024])
                    .unwrap();
                std::io::stderr()
                    .write_all(&vec![b'e'; 1024 * 1024])
                    .unwrap();
            }
            Ok("stdin") => {
                std::io::stdin().read_to_end(&mut Vec::new()).unwrap();
                println!("stdin closed");
            }
            Ok("sleep") => std::thread::sleep(Duration::from_secs(30)),
            Ok("descendant") => {
                std::thread::sleep(Duration::from_millis(200));
                let child = fixture("sleep")
                    .stdout(Stdio::inherit())
                    .stderr(Stdio::inherit())
                    .spawn()
                    .unwrap();
                std::fs::write(
                    std::env::var_os("CC_SWITCH_PROCESS_PID").unwrap(),
                    child.id().to_string(),
                )
                .unwrap();
            }
            _ => {}
        }
    }

    fn fixture(mode: &str) -> Command {
        let mut cmd = Command::new(std::env::current_exe().unwrap());
        cmd.args([
            "--exact",
            "process_output::bounded_output_tests::subprocess_fixture",
            "--nocapture",
        ])
        .env("CC_SWITCH_PROCESS_FIXTURE", mode)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            cmd.creation_flags(CREATE_NO_WINDOW);
        }
        cmd
    }

    fn capture(mode: &str, timeout: Duration) -> Result<std::process::Output, String> {
        let mut cmd = fixture(mode);
        #[cfg(unix)]
        isolate_child_process_group(&mut cmd);
        wait_child_output(
            cmd.spawn().unwrap(),
            CommandDeadline::from_timeout(Some(timeout)),
        )
    }

    #[test]
    fn drains_both_pipes_while_child_is_running() {
        let output = capture("output", Duration::from_secs(10)).unwrap();
        assert!(output.status.success());
        assert!(output.stdout.len() >= 1024 * 1024);
        assert!(output.stderr.len() >= 1024 * 1024);
    }

    #[test]
    fn closes_stdin_before_waiting() {
        let output = capture("stdin", Duration::from_secs(10)).unwrap();
        assert!(output.status.success());
        assert!(String::from_utf8_lossy(&output.stdout).contains("stdin closed"));
    }

    #[test]
    fn times_out_a_running_child() {
        let started = std::time::Instant::now();
        assert!(capture("sleep", Duration::from_millis(300))
            .unwrap_err()
            .contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(10));
    }

    #[test]
    fn times_out_inherited_pipes_after_parent_exits() {
        let dir = tempfile::tempdir().unwrap();
        let pid_file = dir.path().join("pid");
        let mut cmd = fixture("descendant");
        cmd.env("CC_SWITCH_PROCESS_PID", &pid_file);
        #[cfg(unix)]
        isolate_child_process_group(&mut cmd);
        let started = std::time::Instant::now();
        assert!(wait_child_output(
            cmd.spawn().unwrap(),
            CommandDeadline::from_timeout(Some(Duration::from_secs(2)))
        )
        .unwrap_err()
        .contains("timed out"));
        assert!(started.elapsed() < Duration::from_secs(10));
        assert!(
            pid_file.is_file(),
            "fixture must actually spawn the descendant"
        );
    }

    #[test]
    fn preserves_output_up_to_the_limit() {
        for size in [0, 12, MAX_COMMAND_OUTPUT_BYTES] {
            let data = vec![b'x'; size];
            assert_eq!(read_command_output(data.as_slice()).unwrap(), data);
        }
    }

    #[test]
    fn drains_oversized_output_before_reporting_failure() {
        let mut data = std::io::Cursor::new(vec![b'x'; MAX_COMMAND_OUTPUT_BYTES + 20_000]);
        assert!(read_command_output(&mut data)
            .unwrap_err()
            .contains("exceeds"));
        assert_eq!(data.position() as usize, data.get_ref().len());
    }
}
