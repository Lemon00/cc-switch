//! Keep a detached daemon from retaining the invoking script's output pipes.

use std::ffi::c_void;
use std::io;

#[link(name = "kernel32")]
extern "system" {
    fn GetStdHandle(kind: u32) -> *mut c_void;
    fn SetHandleInformation(handle: *mut c_void, mask: u32, flags: u32) -> i32;
}

pub(super) fn prevent_inheritance() -> io::Result<()> {
    const HANDLE_FLAG_INHERIT: u32 = 1;
    // STD_INPUT_HANDLE, STD_OUTPUT_HANDLE and STD_ERROR_HANDLE.
    for kind in [-10i32, -11, -12] {
        // SAFETY: these are the documented standard-handle selectors. The
        // handles remain owned by the process; only inheritance is disabled.
        let handle = unsafe { GetStdHandle(kind as u32) };
        if handle.is_null() || handle == -1isize as *mut c_void {
            continue;
        }
        if unsafe { SetHandleInformation(handle, HANDLE_FLAG_INHERIT, 0) } == 0 {
            return Err(io::Error::last_os_error());
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Read, Write};
    use std::os::windows::process::CommandExt;
    use std::process::{Command, Stdio};
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn redirected_descendant_probe() {
        let Some(marker) = std::env::var_os("CC_SWITCH_STDIO_PROBE") else {
            return;
        };
        prevent_inheritance().unwrap();
        let mut child = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "tests::daemon_readiness_probe_child",
                "--nocapture",
            ])
            .env_remove("CC_SWITCH_STDIO_PROBE")
            .env("CC_SWITCH_TEST_DAEMON_PROBE", "1")
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        println!("parent output remains usable");
        io::stdout().flush().unwrap();
        std::fs::write(marker, child.id().to_string()).unwrap();
        // The real daemon outlives its parent. Here the outer test kills this
        // wrapper first, while the descendant stays alive for up to 10 seconds.
        child.wait().unwrap();
    }

    #[test]
    fn redirected_descendant_does_not_keep_captured_output_open() {
        let marker =
            std::env::temp_dir().join(format!("cc-switch-stdio-{}.pid", std::process::id()));
        let mut wrapper = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "windows_stdio::tests::redirected_descendant_probe",
                "--nocapture",
            ])
            .env("CC_SWITCH_STDIO_PROBE", &marker)
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .creation_flags(0x08000000)
            .spawn()
            .unwrap();
        let mut stdout = wrapper.stdout.take().unwrap();
        let (done_tx, done_rx) = mpsc::channel();
        let reader = thread::spawn(move || {
            let mut output = String::new();
            let result = stdout.read_to_string(&mut output);
            let _ = done_tx.send((result, output));
        });
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while !marker.exists() && std::time::Instant::now() < deadline {
            thread::sleep(Duration::from_millis(10));
        }
        let started = marker.exists();
        wrapper.kill().unwrap();
        wrapper.wait().unwrap();
        let output = done_rx.recv_timeout(Duration::from_secs(2));
        // Always join: on a regression the descendant's bounded sleep closes
        // the inherited pipe, so this test cannot leave a reader thread hung.
        reader.join().unwrap();
        if started {
            std::fs::remove_file(marker).unwrap();
        }
        assert!(started, "descendant did not start");
        let (read, output) = output.expect("descendant retained captured stdout");
        read.unwrap();
        assert!(output.contains("parent output remains usable"), "{output}");
    }
}
