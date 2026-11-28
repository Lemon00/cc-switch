#[cfg(test)]
pub(crate) fn in_isolated_process(test_name: &str) -> bool {
    const CHILD: &str = "CC_SWITCH_LOCK_TEST_CHILD";
    if std::env::var(CHILD).as_deref() == Ok(test_name) {
        return true;
    }
    // Poisoning a process-wide lock must not race unrelated tests in the same
    // binary. Give the child an isolated home as well as isolated static locks.
    let home = tempfile::tempdir().unwrap();
    let config = home.path().join(".cc-switch");
    std::fs::create_dir_all(&config).unwrap();
    std::fs::write(config.join("cc-switch.db"), []).unwrap();
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([test_name, "--exact", "--nocapture", "--test-threads=1"])
        .env(CHILD, test_name)
        .env("CC_SWITCH_TEST_HOME", home.path())
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    // --exact 匹配不到测试时 libtest 也返回 0，测试改名后不能空跑通过。
    assert!(
        output.status.success() && stdout.contains("test result: ok. 1 passed"),
        "isolated poison test failed or did not run:\n{}\n{}",
        stdout,
        String::from_utf8_lossy(&output.stderr)
    );
    false
}
