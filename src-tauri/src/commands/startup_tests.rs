use std::future::Future;
use std::sync::{mpsc, Arc};
use std::task::Context;
use std::time::Duration;

use tauri::Manager;

use crate::app_config::AppType;
use crate::database::Database;
use crate::provider::{Provider, UniversalProvider};
use crate::services::model_pricing::ModelPricingInfo;
use crate::store::AppState;

pub(super) struct TestHome {
    previous: Option<std::ffi::OsString>,
    native_overrides: Vec<(&'static str, Option<std::ffi::OsString>)>,
    _directory: tempfile::TempDir,
}

impl TestHome {
    pub(super) fn new() -> Self {
        let directory = tempfile::tempdir().unwrap();
        let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
        std::env::set_var("CC_SWITCH_TEST_HOME", directory.path());
        let native_overrides = [
            ("KIMI_CODE_HOME", ".kimi-code"),
            ("MINIMAX_DATA_DIR", ".minimax"),
            ("DSH_HOME", ".dsh"),
        ]
        .into_iter()
        .map(|(key, dir)| {
            let previous = std::env::var_os(key);
            std::env::set_var(key, directory.path().join(dir));
            (key, previous)
        })
        .collect();
        crate::settings::update_settings(Default::default())
            .expect("initialize settings for isolated test home");
        Self {
            previous,
            native_overrides,
            _directory: directory,
        }
    }
}

impl Drop for TestHome {
    fn drop(&mut self) {
        for (key, previous) in &self.native_overrides {
            match previous {
                Some(value) => std::env::set_var(key, value),
                None => std::env::remove_var(key),
            }
        }
        match &self.previous {
            Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
            None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
        }
        let _ = crate::settings::reload_settings();
    }
}

/// Hold the same connection mutex as session sync, then poll the actual command.
/// It must yield before the lock is released, including on a single-thread runtime.
/// The watchdog only prevents a regression from hanging the test process.
fn while_connection_locked<F: Future>(db: &Arc<Database>, name: &str, command: F) -> F::Output {
    let (locked_tx, locked_rx) = mpsc::channel();
    let (release_tx, release_rx) = mpsc::channel();
    let db = db.clone();
    let holder = std::thread::spawn(move || {
        db.with_connection_locked_for_test(|| {
            locked_tx.send(()).unwrap();
            release_rx.recv_timeout(Duration::from_secs(5)).is_ok()
        })
    });
    locked_rx.recv_timeout(Duration::from_secs(5)).unwrap();

    let mut command = std::pin::pin!(command);
    let first_poll = command
        .as_mut()
        .poll(&mut Context::from_waker(futures::task::noop_waker_ref()));
    let _ = release_tx.send(());
    let released_by_caller = holder.join().unwrap();
    assert!(
        released_by_caller,
        "{name} blocked its polling thread on the database lock"
    );
    assert!(
        first_poll.is_pending(),
        "{name} must wait asynchronously for the database"
    );

    tauri::async_runtime::block_on(async {
        tokio::time::timeout(Duration::from_secs(5), command)
            .await
            .unwrap_or_else(|_| panic!("{name} did not complete after releasing the database"))
    })
}

/// Representative commands per page a user can reach while session sync still
/// holds the lock: Skills, icon picker, universal providers, endpoint speed test,
/// profiles, network settings, MCP connectivity.
#[test]
#[serial_test::serial]
fn remaining_database_commands_yield_during_session_sync_contention() {
    let _home = TestHome::new();
    let db = Arc::new(Database::memory().unwrap());
    let provider = Provider::with_id(
        "test-provider".into(),
        "Test provider".into(),
        serde_json::json!({}),
        None,
    );
    db.save_provider(&AppType::Claude, &provider).unwrap();
    db.set_current_provider(&AppType::Claude, &provider.id)
        .unwrap();
    let app = tauri::test::mock_builder()
        .manage(AppState::new(db.clone()))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();

    macro_rules! query {
        ($name:ident $(, $arg:expr)*) => {
            while_connection_locked(&db, stringify!($name), super::$name(app.state() $(, $arg)*))
                .unwrap()
        };
    }
    // Skill commands take the state last.
    macro_rules! skill_query {
        ($name:ident $(, $arg:expr)*) => {
            while_connection_locked(&db, stringify!($name), super::$name($($arg,)* app.state()))
                .unwrap()
        };
    }

    assert!(query!(get_custom_icons).is_empty());
    assert_eq!(query!(get_global_proxy_url), None);
    assert!(query!(get_universal_providers).is_empty());
    assert!(query!(get_custom_endpoints, "claude".into(), provider.id.clone()).is_empty());
    let batch = query!(
        start_mcp_connectivity_checks,
        Vec::new(),
        tauri::ipc::Channel::new(|_| Ok(()))
    );
    assert!(!batch.is_empty());

    let profile = query!(create_profile, "Test profile".into(), "claude".into());
    assert_eq!(
        query!(
            update_profile,
            profile.id.clone(),
            Some("Renamed".into()),
            None,
            None
        )
        .name,
        "Renamed"
    );
    query!(delete_profile, profile.id);
    query!(clear_current_profile, "claude".into());

    assert!(skill_query!(get_installed_skills).is_empty());
    skill_query!(scan_unmanaged_skills);
    assert!(skill_query!(get_skill_repos).is_empty());
    let repo = crate::services::skill::SkillRepo {
        owner: "octocat".into(),
        name: "skills".into(),
        branch: "main".into(),
        enabled: true,
    };
    assert!(skill_query!(add_skill_repo, repo));
    assert_eq!(skill_query!(get_skill_repos).len(), 1);
    assert!(skill_query!(
        remove_skill_repo,
        "octocat".into(),
        "skills".into()
    ));

    // Validation errors must still reach the caller after moving work off-thread.
    assert!(tauri::async_runtime::block_on(super::get_custom_endpoints(
        app.state(),
        "invalid".into(),
        provider.id
    ))
    .is_err());
    assert!(tauri::async_runtime::block_on(super::create_profile(
        app.state(),
        "x".into(),
        "nope".into()
    ))
    .is_err());
}

/// Profile and universal-provider mutations are read-modify-write and used to
/// queue on the main thread; now they must queue on their serial locks.
#[test]
#[serial_test::serial]
fn profile_and_universal_mutations_wait_for_their_serial_locks() {
    let _home = TestHome::new();
    let db = Arc::new(Database::memory().unwrap());
    let provider = Provider::with_id(
        "test-provider".into(),
        "Test provider".into(),
        serde_json::json!({}),
        None,
    );
    db.save_provider(&AppType::Claude, &provider).unwrap();
    db.set_current_provider(&AppType::Claude, &provider.id)
        .unwrap();
    let app = tauri::test::mock_builder()
        .manage(AppState::new(db.clone()))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();

    fn while_serialized<F: Future>(
        lock: &'static tokio::sync::Mutex<()>,
        name: &str,
        command: F,
    ) -> F::Output {
        let guard = tauri::async_runtime::block_on(lock.lock());
        let mut command = std::pin::pin!(command);
        let first_poll = command
            .as_mut()
            .poll(&mut Context::from_waker(futures::task::noop_waker_ref()));
        drop(guard);
        assert!(
            first_poll.is_pending(),
            "{name} must wait for the serial lock"
        );
        // tokio hands the released permit straight to the queued waiter.
        assert!(
            lock.try_lock().is_err(),
            "{name} must be queued on the serial lock"
        );
        tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_secs(5), command)
                .await
                .unwrap_or_else(|_| panic!("{name} did not complete after releasing the lock"))
        })
    }

    while_serialized(
        &super::profile::PROFILE_LOCK,
        "clear_current_profile",
        super::clear_current_profile(app.state(), "claude".into()),
    )
    .unwrap();
    let profile = while_serialized(
        &super::profile::PROFILE_LOCK,
        "create_profile",
        super::create_profile(app.state(), "Serialized".into(), "claude".into()),
    )
    .unwrap();
    while_serialized(
        &super::profile::PROFILE_LOCK,
        "delete_profile",
        super::delete_profile(app.state(), profile.id),
    )
    .unwrap();

    let universal: UniversalProvider = serde_json::from_value(serde_json::json!({
        "id": "universal-test",
        "name": "Universal",
        "providerType": "custom",
        "apps": {},
        "baseUrl": "https://example.invalid/v1",
        "apiKey": "test",
    }))
    .unwrap();
    let state = app.state::<AppState>();
    assert!(while_serialized(
        &super::provider::UNIVERSAL_LOCK,
        "upsert_universal_provider",
        super::provider::universal_mutation(&state, move |state| {
            crate::services::ProviderService::upsert_universal(state, universal)
                .map_err(|e| e.to_string())
        }),
    )
    .unwrap());
    assert!(db
        .get_universal_provider("universal-test")
        .unwrap()
        .is_some());
}
