//! Background auto sync shared by the WebDAV and S3 backends: local DB
//! changes are debounced, then the snapshot is uploaded under the global
//! sync lock. Each backend supplies its settings and upload call through
//! [`AutoSyncBackend`].

use std::future::Future;
use std::marker::PhantomData;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::{Duration, Instant};

use serde_json::{json, Value};
use tokio::sync::mpsc::error::TrySendError;
use tokio::sync::mpsc::{channel, Receiver, Sender};

use crate::database::Database;
use crate::error::AppError;
use crate::events::EventSink;
use crate::services::sync_protocol::{run_with_sync_lock, should_trigger_auto_sync_for_table};
use crate::settings::WebDavSyncStatus;

const AUTO_SYNC_DEBOUNCE_MS: u64 = 1000;
const MAX_AUTO_SYNC_WAIT_MS: u64 = 10_000;
const MAX_RETRY_DELAY_MS: u64 = 300_000;
#[cfg(not(test))]
const INITIAL_RETRY_DELAY_MS: u64 = 5_000;
#[cfg(test)]
const INITIAL_RETRY_DELAY_MS: u64 = 50;

#[derive(Default)]
struct PendingSync {
    generation: u64,
    marker: Option<std::path::PathBuf>,
}

pub trait AutoSyncBackend: Send + Sync + 'static {
    type Settings: Send + 'static;
    /// Log prefix, e.g. `WebDAV`.
    const NAME: &'static str;
    /// Event the frontend listens to for auto sync results.
    const STATUS_EVENT: &'static str;

    fn state() -> &'static AutoSyncState;
    fn load() -> Option<Self::Settings>;
    fn wants_auto_sync(settings: &Self::Settings) -> bool;
    fn status_mut(settings: &mut Self::Settings) -> &mut WebDavSyncStatus;
    fn store_status(status: WebDavSyncStatus) -> Result<(), AppError>;
    fn upload<'a>(
        db: &'a Database,
        settings: &'a mut Self::Settings,
    ) -> impl Future<Output = Result<Value, AppError>> + Send + 'a;
}

/// Per-backend worker state.
pub struct AutoSyncState {
    tx: OnceLock<Sender<String>>,
    suppress_depth: AtomicUsize,
    pending: Mutex<PendingSync>,
}

impl AutoSyncState {
    pub const fn new() -> Self {
        Self {
            tx: OnceLock::new(),
            suppress_depth: AtomicUsize::new(0),
            pending: Mutex::new(PendingSync {
                generation: 0,
                marker: None,
            }),
        }
    }
}

fn pending_marker<B: AutoSyncBackend>() -> std::path::PathBuf {
    crate::config::get_app_config_dir().join(format!("auto-sync-{}.pending", B::NAME))
}

fn mark_pending<B: AutoSyncBackend>() -> Result<(), AppError> {
    let mut pending = B::state().pending.lock()?;
    pending.generation = pending.generation.wrapping_add(1);
    let path = pending_marker::<B>();
    if pending.marker.as_ref() != Some(&path) {
        crate::config::atomic_write_private(&path, b"pending\n")?;
        pending.marker = Some(path);
    }
    Ok(())
}

fn clear_pending<B: AutoSyncBackend>(generation: u64) -> Result<(), AppError> {
    let mut pending = B::state().pending.lock()?;
    if pending.generation != generation {
        return Ok(());
    }
    if let Some(path) = &pending.marker {
        match std::fs::remove_file(path) {
            Ok(()) => {}
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::io(path, error)),
        }
    }
    pending.marker = None;
    Ok(())
}

fn recover_pending<B: AutoSyncBackend>(tx: &Sender<String>) -> Result<(), AppError> {
    let mut pending = B::state().pending.lock()?;
    let path = pending_marker::<B>();
    if path
        .try_exists()
        .map_err(|error| AppError::io(&path, error))?
    {
        pending.marker = Some(path);
        let _ = enqueue_change_signal(tx, "restart-recovery");
    }
    Ok(())
}

fn retry_delay(attempt: u32) -> Duration {
    let base = INITIAL_RETRY_DELAY_MS.saturating_mul(1u64 << attempt.min(16));
    let base = base.min(MAX_RETRY_DELAY_MS);
    let jitter = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .subsec_nanos() as u64
        % (base / 4 + 1);
    Duration::from_millis((base + jitter).min(MAX_RETRY_DELAY_MS))
}

/// While alive, DB changes don't queue an auto upload for backend `B`.
pub struct SuppressionGuard<B: AutoSyncBackend>(PhantomData<B>);

impl<B: AutoSyncBackend> SuppressionGuard<B> {
    pub fn new() -> Self {
        B::state().suppress_depth.fetch_add(1, Ordering::SeqCst);
        Self(PhantomData)
    }
}

impl<B: AutoSyncBackend> Drop for SuppressionGuard<B> {
    fn drop(&mut self) {
        let _ =
            B::state()
                .suppress_depth
                .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |value| {
                    Some(value.saturating_sub(1))
                });
    }
}

pub fn is_suppressed<B: AutoSyncBackend>() -> bool {
    B::state().suppress_depth.load(Ordering::SeqCst) > 0
}

fn enqueue_change_signal(tx: &Sender<String>, table: &str) -> bool {
    match tx.try_send(table.to_string()) {
        Ok(()) => true,
        Err(TrySendError::Full(_)) | Err(TrySendError::Closed(_)) => false,
    }
}

fn auto_sync_wait_duration(started_at: Instant, now: Instant) -> Option<Duration> {
    let max_wait = Duration::from_millis(MAX_AUTO_SYNC_WAIT_MS);
    let debounce = Duration::from_millis(AUTO_SYNC_DEBOUNCE_MS);
    let elapsed = now.saturating_duration_since(started_at);
    if elapsed >= max_wait {
        return None;
    }
    Some(debounce.min(max_wait - elapsed))
}

fn load_if_auto_sync<B: AutoSyncBackend>() -> Option<B::Settings> {
    B::load().filter(B::wants_auto_sync)
}

fn persist_auto_sync_error<B: AutoSyncBackend>(settings: &mut B::Settings, error: &AppError) {
    let status = B::status_mut(settings);
    status.last_error = Some(error.to_string());
    status.last_error_source = Some("auto".to_string());
    let _ = B::store_status(status.clone());
}

fn emit_auto_sync_status_updated<B: AutoSyncBackend>(
    sink: &dyn EventSink,
    status: &str,
    error: Option<&str>,
) {
    let payload = match error {
        Some(message) => json!({
            "source": "auto",
            "status": status,
            "error": message,
        }),
        None => json!({
            "source": "auto",
            "status": status,
        }),
    };

    sink.emit(B::STATUS_EVENT, payload);
}

async fn run_auto_sync_upload<B: AutoSyncBackend>(
    db: &Database,
    sink: &dyn EventSink,
) -> Result<bool, AppError> {
    if load_if_auto_sync::<B>().is_none() {
        return Ok(false);
    }
    // Re-read the settings under the sync lock so a "clear config" or "turn
    // off auto sync" that got the lock first is honoured instead of uploading
    // with the stale credentials captured before waiting.
    let outcome = run_with_sync_lock(async {
        let Some(mut sync_settings) = load_if_auto_sync::<B>() else {
            return Ok(None);
        };
        let result = B::upload(db, &mut sync_settings).await;
        Ok(Some((sync_settings, result)))
    })
    .await?;
    let Some((mut sync_settings, result)) = outcome else {
        return Ok(false);
    };
    match result {
        Ok(_) => {
            emit_auto_sync_status_updated::<B>(sink, "success", None);
            Ok(true)
        }
        Err(err) => {
            persist_auto_sync_error::<B>(&mut sync_settings, &err);
            emit_auto_sync_status_updated::<B>(sink, "error", Some(&err.to_string()));
            Err(err)
        }
    }
}

pub fn notify_db_changed<B: AutoSyncBackend>(table: &str) {
    if is_suppressed::<B>() {
        return;
    }
    if !should_trigger_auto_sync_for_table(table) {
        return;
    }
    if load_if_auto_sync::<B>().is_none() {
        return;
    }
    if let Err(error) = mark_pending::<B>() {
        log::warn!(
            "[{}][AutoSync] Could not persist pending upload: {error}",
            B::NAME
        );
    }
    let Some(tx) = B::state().tx.get() else {
        return;
    };
    let _ = enqueue_change_signal(tx, table);
}

pub fn start_worker<B: AutoSyncBackend>(db: Arc<Database>, sink: Arc<dyn EventSink>) {
    if B::state().tx.get().is_some() {
        return;
    }

    // Buffer size 1 is enough: we only need "dirty" signals, not every event.
    let (tx, rx) = channel::<String>(1);
    if B::state().tx.set(tx.clone()).is_err() {
        return;
    }
    if let Err(error) = recover_pending::<B>(&tx) {
        log::warn!(
            "[{}][AutoSync] Could not recover pending upload: {error}",
            B::NAME
        );
    }

    // NOTE: must not use tokio::task::spawn here: start_worker is called from
    // GUI setup, which has no ambient tokio runtime (that panicked on startup).
    crate::background::spawn_background_worker(
        match B::NAME {
            "WebDAV" => "webdav-auto-sync",
            "S3" => "s3-auto-sync",
            _ => "auto-sync",
        },
        async move {
            run_worker_loop::<B>(db, rx, sink).await;
        },
    );
}

async fn run_worker_loop<B: AutoSyncBackend>(
    db: Arc<Database>,
    mut rx: Receiver<String>,
    sink: Arc<dyn EventSink>,
) {
    let mut retry = None;
    let mut attempt = 0;
    loop {
        let first_table = match retry.take() {
            Some(delay) => match tokio::time::timeout(delay, rx.recv()).await {
                Ok(signal) => signal,
                Err(_) => Some("retry".to_string()),
            },
            None => rx.recv().await,
        };
        let Some(first_table) = first_table else {
            return;
        };
        let started_at = Instant::now();
        let mut merged_count = 1usize;

        while let Some(wait_for) = auto_sync_wait_duration(started_at, Instant::now()) {
            let timeout = tokio::time::timeout(wait_for, rx.recv()).await;

            match timeout {
                Ok(Some(_)) => merged_count += 1,
                Ok(None) => return,
                Err(_) => break,
            }
        }

        log::debug!(
            "[{}][AutoSync] Triggered by table={first_table}, merged_changes={merged_count}",
            B::NAME
        );

        let generation = match B::state().pending.lock() {
            Ok(pending) => pending.generation,
            Err(error) => {
                log::error!("[{}][AutoSync] Pending state unavailable: {error}", B::NAME);
                return;
            }
        };
        match run_auto_sync_upload::<B>(&db, sink.as_ref()).await {
            Ok(true) => {
                if let Err(error) = clear_pending::<B>(generation) {
                    log::warn!(
                        "[{}][AutoSync] Could not clear pending upload: {error}",
                        B::NAME
                    );
                }
                attempt = 0;
            }
            Ok(false) => {
                retry = Some(Duration::from_millis(MAX_RETRY_DELAY_MS));
            }
            Err(error) => {
                if matches!(error, AppError::RetryableSync(_)) {
                    retry = Some(retry_delay(attempt));
                    attempt = attempt.saturating_add(1);
                }
                log::warn!("[{}][AutoSync] Upload failed: {error}", B::NAME);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    struct RetryBackend;
    static RETRY_STATE: super::AutoSyncState = super::AutoSyncState::new();
    static RETRY_CALLS: AtomicUsize = AtomicUsize::new(0);
    static RETRY_FAILURES: AtomicUsize = AtomicUsize::new(0);
    static RETRY_ENABLED: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(true);
    static RETRY_TERMINAL: std::sync::atomic::AtomicBool =
        std::sync::atomic::AtomicBool::new(false);

    impl AutoSyncBackend for RetryBackend {
        type Settings = crate::settings::WebDavSyncStatus;
        const NAME: &'static str = "RetryTest";
        const STATUS_EVENT: &'static str = "retry-test";
        fn state() -> &'static super::AutoSyncState {
            &RETRY_STATE
        }
        fn load() -> Option<Self::Settings> {
            Some(Default::default())
        }
        fn wants_auto_sync(_: &Self::Settings) -> bool {
            RETRY_ENABLED.load(Ordering::SeqCst)
        }
        fn status_mut(settings: &mut Self::Settings) -> &mut Self::Settings {
            settings
        }
        fn store_status(_: Self::Settings) -> Result<(), AppError> {
            Ok(())
        }
        async fn upload(
            _: &Database,
            _: &mut Self::Settings,
        ) -> Result<serde_json::Value, AppError> {
            let call = RETRY_CALLS.fetch_add(1, Ordering::SeqCst);
            if RETRY_TERMINAL.load(Ordering::SeqCst) {
                return Err(AppError::Conflict("remote changed".into()));
            }
            if call < RETRY_FAILURES.load(Ordering::SeqCst) {
                return Err(AppError::RetryableSync(Box::new(AppError::Message(
                    "offline".into(),
                ))));
            }
            Ok(serde_json::Value::Null)
        }
    }

    #[tokio::test]
    #[serial]
    async fn retry_recovers_after_restart_without_another_database_write() {
        let _home = IsolatedHome::new();
        RETRY_CALLS.store(0, Ordering::SeqCst);
        RETRY_FAILURES.store(1, Ordering::SeqCst);
        RETRY_ENABLED.store(true, Ordering::SeqCst);
        RETRY_TERMINAL.store(false, Ordering::SeqCst);
        super::mark_pending::<RetryBackend>().unwrap();
        RETRY_STATE.pending.lock().unwrap().marker = None;
        let (tx, rx) = channel(1);
        super::recover_pending::<RetryBackend>(&tx).unwrap();
        let worker = tokio::spawn(run_worker_loop::<RetryBackend>(
            Arc::new(Database::memory().unwrap()),
            rx,
            Arc::new(NoOpEventSink),
        ));
        tokio::time::timeout(Duration::from_secs(8), async {
            while super::pending_marker::<RetryBackend>().exists() {
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        assert_eq!(RETRY_CALLS.load(Ordering::SeqCst), 2);
        drop(tx);
        worker.await.unwrap();
    }

    #[tokio::test]
    #[serial]
    async fn terminal_errors_do_not_retry_and_disabling_prevents_upload() {
        let _home = IsolatedHome::new();
        RETRY_CALLS.store(0, Ordering::SeqCst);
        RETRY_ENABLED.store(true, Ordering::SeqCst);
        RETRY_TERMINAL.store(true, Ordering::SeqCst);
        super::mark_pending::<RetryBackend>().unwrap();
        let (tx, rx) = channel(1);
        tx.send("providers".to_string()).await.unwrap();
        let worker = tokio::spawn(run_worker_loop::<RetryBackend>(
            Arc::new(Database::memory().unwrap()),
            rx,
            Arc::new(NoOpEventSink),
        ));
        tokio::time::sleep(Duration::from_millis(2400)).await;
        assert_eq!(RETRY_CALLS.load(Ordering::SeqCst), 1);
        assert!(super::pending_marker::<RetryBackend>().exists());
        RETRY_ENABLED.store(false, Ordering::SeqCst);
        tx.send("providers".to_string()).await.unwrap();
        tokio::time::sleep(Duration::from_millis(1300)).await;
        assert_eq!(RETRY_CALLS.load(Ordering::SeqCst), 1);
        drop(tx);
        worker.await.unwrap();
    }

    #[test]
    #[serial]
    fn pending_generation_preserves_changes_during_upload_and_retry_is_bounded() {
        let _home = IsolatedHome::new();
        super::mark_pending::<RetryBackend>().unwrap();
        let generation = RETRY_STATE.pending.lock().unwrap().generation;
        super::mark_pending::<RetryBackend>().unwrap();
        super::clear_pending::<RetryBackend>(generation).unwrap();
        assert!(super::pending_marker::<RetryBackend>().exists());
        for attempt in [0, 1, 10, 32, u32::MAX] {
            assert!(
                super::retry_delay(attempt) <= Duration::from_millis(super::MAX_RETRY_DELAY_MS)
            );
        }
    }
    use super::{
        auto_sync_wait_duration, run_worker_loop, AutoSyncBackend, AUTO_SYNC_DEBOUNCE_MS,
        MAX_AUTO_SYNC_WAIT_MS,
    };
    use crate::database::Database;
    use crate::error::AppError;
    use crate::events::{EventSink, NoOpEventSink};
    use crate::services::s3_auto_sync::S3AutoSync;
    use crate::services::sync_protocol::sync_mutex;
    use crate::services::webdav_auto_sync::WebDavAutoSync;
    use crate::settings::{self, AppSettings, S3SyncSettings, WebDavSyncSettings};
    use serial_test::serial;
    use std::io::{BufRead, BufReader, Read, Write};
    use std::sync::atomic::{AtomicUsize, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};
    use tokio::sync::mpsc::channel;

    #[test]
    fn max_wait_caps_flush_latency_for_continuous_events() {
        let started = Instant::now();
        let later = started + Duration::from_millis(MAX_AUTO_SYNC_WAIT_MS + 1);
        assert!(auto_sync_wait_duration(started, later).is_none());
    }

    #[test]
    fn service_layer_does_not_depend_on_commands_layer() {
        let needle = ["crate", "commands", ""].join("::");
        for source in [
            include_str!("auto_sync.rs"),
            include_str!("webdav_auto_sync.rs"),
            include_str!("s3_auto_sync.rs"),
        ] {
            assert!(
                !source.contains(&needle),
                "services layer should not depend on commands layer"
            );
        }
    }

    /// A remote that answers 200 to every request and counts them.
    fn counting_remote() -> (String, Arc<AtomicUsize>) {
        let listener = std::net::TcpListener::bind("127.0.0.1:0").expect("bind local listener");
        let url = format!("http://{}", listener.local_addr().expect("local addr"));
        let hits = Arc::new(AtomicUsize::new(0));
        let hits_in_thread = Arc::clone(&hits);
        std::thread::spawn(move || {
            for stream in listener.incoming() {
                let Ok(mut stream) = stream else { continue };
                hits_in_thread.fetch_add(1, Ordering::SeqCst);
                let mut reader = BufReader::new(stream.try_clone().expect("clone stream"));
                let mut body_len = 0;
                loop {
                    let mut line = String::new();
                    if reader.read_line(&mut line).unwrap_or(0) == 0 || line == "\r\n" {
                        break;
                    }
                    if let Some(value) = line.to_ascii_lowercase().strip_prefix("content-length:") {
                        body_len = value.trim().parse().unwrap_or(0);
                    }
                }
                let _ = reader.read_exact(&mut vec![0; body_len]);
                let _ = stream.write_all(
                    b"HTTP/1.1 200 OK\r\nContent-Length: 0\r\nConnection: close\r\n\r\n",
                );
            }
        });
        (url, hits)
    }

    fn webdav_settings(remote: &str, enabled: bool, auto_sync: bool) -> WebDavSyncSettings {
        WebDavSyncSettings {
            enabled,
            auto_sync,
            base_url: remote.to_string(),
            username: "alice".to_string(),
            password: "secret".to_string(),
            ..WebDavSyncSettings::default()
        }
    }

    fn s3_settings(remote: &str, enabled: bool, auto_sync: bool) -> S3SyncSettings {
        S3SyncSettings {
            enabled,
            auto_sync,
            region: "us-east-1".to_string(),
            bucket: "bucket".to_string(),
            access_key_id: "AKID".to_string(),
            secret_access_key: "SECRET".to_string(),
            endpoint: remote.to_string(),
            ..S3SyncSettings::default()
        }
    }

    /// What the user does while the auto upload waits for the sync lock.
    enum UserChange {
        Nothing,
        /// Saves the same config with these `(enabled, auto_sync)` flags.
        Save(bool, bool),
        /// Clears the saved config.
        Clear,
    }

    /// Points settings at a fresh home for one test and puts the old one back.
    struct IsolatedHome {
        _dir: tempfile::TempDir,
        previous: Option<std::ffi::OsString>,
    }

    impl IsolatedHome {
        fn new() -> Self {
            let dir = tempfile::tempdir().expect("create test home");
            let previous = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            settings::update_settings(AppSettings::default()).expect("reset settings");
            Self {
                _dir: dir,
                previous,
            }
        }
    }

    impl Drop for IsolatedHome {
        fn drop(&mut self) {
            let _ = settings::update_settings(AppSettings::default());
            match self.previous.take() {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    /// A DB change queues an auto upload while another sync operation holds
    /// the lock. While it waits, the user makes `change`. Returns how many
    /// requests reached the remote.
    async fn requests_after_queued_upload<B: AutoSyncBackend>(
        store: fn(Option<B::Settings>) -> Result<(), AppError>,
        settings_for: fn(&str, bool, bool) -> B::Settings,
        change: UserChange,
    ) -> usize {
        let _home = IsolatedHome::new();
        let (remote, hits) = counting_remote();
        store(Some(settings_for(&remote, true, true))).expect("seed settings");

        let db = Arc::new(Database::memory().expect("memory db"));
        let sink: Arc<dyn EventSink> = Arc::new(NoOpEventSink);
        let (tx, rx) = channel::<String>(1);

        let other_sync = sync_mutex().lock().await;
        let worker = tokio::spawn(run_worker_loop::<B>(db, rx, sink));
        tx.send("providers".to_string())
            .await
            .expect("queue change");
        // Past the debounce, so the upload is queued on the sync lock.
        tokio::time::sleep(Duration::from_millis(AUTO_SYNC_DEBOUNCE_MS + 300)).await;

        match change {
            UserChange::Nothing => {}
            UserChange::Save(enabled, auto_sync) => {
                store(Some(settings_for(&remote, enabled, auto_sync))).expect("save settings")
            }
            UserChange::Clear => store(None).expect("clear settings"),
        }
        drop(other_sync);
        drop(tx);
        tokio::time::timeout(Duration::from_secs(10), worker)
            .await
            .expect("worker should finish")
            .expect("worker should not panic");
        hits.load(Ordering::SeqCst)
    }

    macro_rules! queued_upload_tests {
        ($name:ident, $backend:ty, $store:path, $settings:path) => {
            mod $name {
                use super::*;

                async fn requests(change: UserChange) -> usize {
                    requests_after_queued_upload::<$backend>($store, $settings, change).await
                }

                #[tokio::test]
                #[serial]
                async fn queued_upload_runs_when_settings_are_unchanged() {
                    let requests = requests(UserChange::Nothing).await;
                    assert!(requests > 0, "the queued upload should reach the remote");
                }

                #[tokio::test]
                #[serial]
                async fn queued_upload_is_dropped_after_auto_sync_is_turned_off() {
                    let requests = requests(UserChange::Save(true, false)).await;
                    assert_eq!(
                        requests, 0,
                        "nothing may be uploaded after auto sync is off"
                    );
                }

                #[tokio::test]
                #[serial]
                async fn queued_upload_is_dropped_after_sync_is_disabled() {
                    let requests = requests(UserChange::Save(false, true)).await;
                    assert_eq!(
                        requests, 0,
                        "nothing may be uploaded after sync is disabled"
                    );
                }
            }
        };
    }

    queued_upload_tests!(
        webdav,
        WebDavAutoSync,
        settings::set_webdav_sync_settings,
        webdav_settings
    );
    queued_upload_tests!(s3, S3AutoSync, settings::set_s3_sync_settings, s3_settings);

    /// "Clear config" exists only for WebDAV (`webdav_sync_clear_settings`).
    #[tokio::test]
    #[serial]
    async fn webdav_queued_upload_is_dropped_after_config_is_cleared() {
        let requests = requests_after_queued_upload::<WebDavAutoSync>(
            settings::set_webdav_sync_settings,
            webdav_settings,
            UserChange::Clear,
        )
        .await;
        assert_eq!(
            requests, 0,
            "nothing may be uploaded after the config is cleared"
        );
    }
}
