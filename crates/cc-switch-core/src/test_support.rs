//! Isolated home for upstream regression tests. Callers changing the environment run serially.
use std::{ffi::OsString, path::Path};

pub(crate) struct TestHome {
    previous: Option<OsString>,
    dir: tempfile::TempDir,
}

impl TestHome {
    pub(crate) fn new() -> Self {
        let dir = tempfile::tempdir().expect("create test home");
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", dir.path());
        let settings_dir = crate::config::get_app_config_dir();
        std::fs::create_dir_all(&settings_dir).expect("create isolated settings directory");
        std::fs::write(settings_dir.join("settings.json"), "{}").expect("write isolated settings");
        let guard = Self { previous, dir };
        crate::settings::reload_settings().expect("reload isolated settings");
        guard
    }

    pub(crate) fn path(&self) -> &Path {
        self.dir.path()
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        match self.previous.take() {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
        let _ = crate::settings::reload_settings();
    }
}
