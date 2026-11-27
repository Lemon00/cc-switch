use std::process::{Command, Stdio};

#[test]
fn tui_rejects_redirected_io_and_extra_arguments_before_opening_database() {
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join("config");
    for (args, expected) in [
        (vec!["tui"], "需要交互式终端"),
        (vec!["--app", "agy", "tui"], "需要交互式终端"),
        (vec!["tui", "extra"], "无效命令或多余参数"),
    ] {
        let output = Command::new(env!("CARGO_BIN_EXE_cc-switch-cli"))
            .args(args)
            .env("CC_SWITCH_TEST_HOME", home.path())
            .env("CC_SWITCH_CONFIG_DIR", &config)
            .stdin(Stdio::null())
            .output()
            .unwrap();
        assert!(!output.status.success());
        assert!(
            String::from_utf8_lossy(&output.stderr).contains(expected),
            "{:?}",
            output
        );
        assert!(!config.join("cc-switch.db").exists());
        assert!(!config.join("cli.json").exists());
    }
}
