//! Failure and cross-connection regression tests for persistence guarantees.
use super::{changes::NoOpDatabaseChangeObserver, Database, SCHEMA_VERSION};
use crate::{app::AppType, error::AppError, provider::Provider};
use rusqlite::Connection;
use std::sync::{Arc, Mutex};

fn file_database(path: &std::path::Path) -> Database {
    let conn = Connection::open(path).unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON").unwrap();
    conn.busy_timeout(std::time::Duration::from_secs(5))
        .unwrap();
    Database::initialize_schema_on_conn(&conn).unwrap();
    Database {
        route_revisions: Default::default(),
        conn: Mutex::new(conn),
        log_count_cache: Mutex::new(None),
        change_observer: Arc::new(NoOpDatabaseChangeObserver),
    }
}

fn provider(db: &Database) {
    db.save_provider(
        &AppType::Claude,
        &Provider::with_id("p".into(), "Provider".into(), serde_json::json!({}), None),
    )
    .unwrap();
}

#[test]
fn future_schema_is_rejected_before_compatibility_writes() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch(
        "CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
        INSERT INTO settings VALUES ('current_profile_id', 'preserve');",
    )
    .unwrap();
    Database::set_user_version(&conn, SCHEMA_VERSION + 1).unwrap();
    assert!(Database::create_tables_on_conn(&conn).is_err());
    assert!(Database::initialize_schema_on_conn(&conn).is_err());
    assert!(!Database::table_exists(&conn, "providers").unwrap());
    assert_eq!(
        conn.query_row("SELECT value FROM settings", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "preserve"
    );
    assert_eq!(
        Database::get_user_version(&conn).unwrap(),
        SCHEMA_VERSION + 1
    );
    assert!(conn.is_autocommit());
}

#[test]
fn negative_schema_version_is_rejected_without_open_transaction() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA user_version = -1").unwrap();
    assert!(Database::initialize_schema_on_conn(&conn).is_err());
    assert!(!Database::table_exists(&conn, "providers").unwrap());
    assert!(conn.is_autocommit());
}

#[test]
fn duplicate_legacy_endpoints_are_migrated_before_unique_index_creation() {
    let db = Database::memory().unwrap();
    provider(&db);
    let conn = db.conn.lock().unwrap();
    conn.execute_batch(
        "DROP INDEX idx_provider_endpoints_unique;
        INSERT INTO provider_endpoints (id, provider_id, app_type, url, added_at)
        VALUES (1, 'p', 'claude', 'https://example.test', 123),
               (2, 'p', 'claude', 'https://example.test', 456);
        PRAGMA user_version = 24;",
    )
    .unwrap();
    Database::initialize_schema_on_conn(&conn).unwrap();
    let endpoints: (i64, i64, i64) = conn
        .query_row(
            "SELECT COUNT(*), id, added_at FROM provider_endpoints",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .unwrap();
    assert_eq!(endpoints, (1, 1, 123));
    Database::initialize_schema_on_conn(&conn).unwrap();
    assert_eq!(Database::get_user_version(&conn).unwrap(), SCHEMA_VERSION);
    assert!(conn
        .execute(
            "INSERT INTO provider_endpoints (provider_id, app_type, url)
        VALUES ('p', 'claude', 'https://example.test')",
            []
        )
        .is_err());
}

#[test]
fn late_initialization_failure_rolls_back_repairs_migrations_and_version() {
    let conn = Connection::open_in_memory().unwrap();
    Database::create_tables_on_conn(&conn).unwrap();
    conn.execute_batch(
        "DROP TABLE profiles;
        INSERT INTO settings VALUES ('current_profile_id', 'legacy');
        CREATE TRIGGER reject_seed BEFORE INSERT ON model_pricing
        BEGIN SELECT RAISE(ABORT, 'seed failed'); END;
        PRAGMA user_version = 24;",
    )
    .unwrap();
    assert!(Database::initialize_schema_on_conn(&conn).is_err());
    assert!(conn.is_autocommit());
    assert_eq!(Database::get_user_version(&conn).unwrap(), 24);
    assert!(!Database::table_exists(&conn, "profiles").unwrap());
    assert_eq!(
        conn.query_row("SELECT key FROM settings", [], |r| r.get::<_, String>(0))
            .unwrap(),
        "current_profile_id"
    );
    conn.execute_batch("DROP TRIGGER reject_seed").unwrap();
    Database::initialize_schema_on_conn(&conn).unwrap();
    assert_eq!(Database::get_user_version(&conn).unwrap(), SCHEMA_VERSION);
    assert!(Database::table_exists(&conn, "profiles").unwrap());
}

#[test]
fn failed_legacy_skill_snapshot_preserves_old_table_and_version() {
    let conn = Connection::open_in_memory().unwrap();
    Database::create_tables_on_conn(&conn).unwrap();
    conn.execute_batch("DROP TABLE skills;
        CREATE TABLE skills (directory TEXT, app_type TEXT, installed INTEGER, installed_at INTEGER);
        INSERT INTO skills VALUES ('my-skill', 'claude', 1, 100);
        CREATE TRIGGER reject_snapshot BEFORE INSERT ON settings
        WHEN NEW.key = 'skills_ssot_migration_snapshot'
        BEGIN SELECT RAISE(ABORT, 'snapshot failed'); END;
        PRAGMA user_version = 2;").unwrap();
    assert!(Database::initialize_schema_on_conn(&conn).is_err());
    assert!(conn.is_autocommit());
    assert_eq!(Database::get_user_version(&conn).unwrap(), 2);
    assert_eq!(
        conn.query_row("SELECT directory FROM skills", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "my-skill"
    );
    let count: i64 = conn
        .query_row(
            "SELECT COUNT(*) FROM settings WHERE key LIKE 'skills_ssot_migration_%'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert_eq!(count, 0);
    conn.execute_batch("DROP TRIGGER reject_snapshot").unwrap();
    Database::initialize_schema_on_conn(&conn).unwrap();
    let snapshot: String = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'skills_ssot_migration_snapshot'",
            [],
            |r| r.get(0),
        )
        .unwrap();
    assert!(snapshot.contains("my-skill"));
}

#[test]
fn profile_compatibility_repair_preserves_existing_scoped_choice() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);
        INSERT INTO settings VALUES ('current_profile_id', 'old'), ('current_profile_id_claude', 'new');").unwrap();
    Database::create_tables_on_conn(&conn).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT value FROM settings WHERE key = 'current_profile_id_claude'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "new"
    );
}

#[test]
fn unreadable_legacy_proxy_values_abort_rebuild_without_replacing_them() {
    let conn = Connection::open_in_memory().unwrap();
    Database::create_tables_on_conn(&conn).unwrap();
    conn.execute_batch(
        "DROP TABLE proxy_config;
        CREATE TABLE proxy_config (id INTEGER PRIMARY KEY, listen_address TEXT,
            listen_port INTEGER, max_retries INTEGER, enable_logging INTEGER,
            streaming_first_byte_timeout INTEGER, streaming_idle_timeout INTEGER,
            non_streaming_timeout INTEGER);
        INSERT INTO proxy_config VALUES (1, '127.0.0.1', 'unreadable', 3, 1, 30, 60, 300);
        PRAGMA user_version = 2;",
    )
    .unwrap();
    assert!(Database::initialize_schema_on_conn(&conn).is_err());
    assert!(conn.is_autocommit());
    assert_eq!(Database::get_user_version(&conn).unwrap(), 2);
    assert_eq!(
        conn.query_row("SELECT listen_port FROM proxy_config", [], |r| r
            .get::<_, String>(0))
            .unwrap(),
        "unreadable"
    );
    assert!(!Database::has_column(&conn, "proxy_config", "app_type").unwrap());
}

#[test]
fn savepoint_commit_failure_rolls_back_and_releases_transaction() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("PRAGMA foreign_keys = ON;
        CREATE TABLE parent (id INTEGER PRIMARY KEY);
        CREATE TABLE child (parent_id INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);").unwrap();
    let result = Database::in_savepoint(&conn, "deferred", || {
        conn.execute("INSERT INTO child VALUES (1)", [])?;
        Ok(())
    });
    assert!(result.is_err());
    assert!(conn.is_autocommit());
    assert_eq!(
        conn.query_row("SELECT COUNT(*) FROM child", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        0
    );
    conn.execute_batch("BEGIN; INSERT INTO parent VALUES (1); COMMIT;")
        .unwrap();
}

#[test]
fn savepoint_unwind_preserves_outer_transaction() {
    let conn = Connection::open_in_memory().unwrap();
    conn.execute_batch("CREATE TABLE entries (id INTEGER); BEGIN; INSERT INTO entries VALUES (1);")
        .unwrap();
    let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _ = Database::in_savepoint::<()>(&conn, "nested", || {
            conn.execute("INSERT INTO entries VALUES (2)", [])?;
            panic!("injected failure");
        });
    }));
    assert!(result.is_err());
    assert!(!conn.is_autocommit());
    conn.execute_batch("COMMIT").unwrap();
    assert_eq!(
        conn.query_row("SELECT SUM(id) FROM entries", [], |r| r.get::<_, i64>(0))
            .unwrap(),
        1
    );
}

#[test]
fn memory_database_has_current_version_and_keeps_custom_prices_on_reopen() {
    let db = Database::memory().unwrap();
    let conn = db.conn.lock().unwrap();
    assert_eq!(Database::get_user_version(&conn).unwrap(), SCHEMA_VERSION);
    conn.execute("INSERT INTO model_pricing (model_id, display_name, input_cost_per_million, output_cost_per_million)
        VALUES ('custom-model', 'Custom', '123', '456')", []).unwrap();
    Database::initialize_schema_on_conn(&conn).unwrap();
    assert_eq!(
        conn.query_row(
            "SELECT input_cost_per_million FROM model_pricing WHERE model_id = 'custom-model'",
            [],
            |r| r.get::<_, String>(0)
        )
        .unwrap(),
        "123"
    );
}

#[test]
fn default_seed_batches_roll_back_if_completion_marker_cannot_be_written() {
    let db = Database::memory().unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TRIGGER reject_initialized BEFORE INSERT ON settings
        WHEN NEW.key IN ('official_providers_seeded', 'default_skill_repos_initialized')
        BEGIN SELECT RAISE(ABORT, 'marker failed'); END;",
        )
        .unwrap();
    assert!(db.init_default_official_providers().is_err());
    assert!(db.is_providers_empty().unwrap());
    assert!(db.init_default_skill_repos().is_err());
    assert!(db.get_skill_repos().unwrap().is_empty());
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_initialized")
        .unwrap();
    assert_eq!(
        db.init_default_official_providers().unwrap(),
        super::dao::providers_seed::OFFICIAL_SEEDS.len()
    );
    assert_eq!(
        db.init_default_skill_repos().unwrap(),
        crate::skill_model::SkillStore::default().repos.len()
    );
    assert_eq!(db.init_default_official_providers().unwrap(), 0);
    assert_eq!(db.init_default_skill_repos().unwrap(), 0);
}

#[test]
fn independent_connections_seed_exactly_once() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shared.db");
    let first = file_database(&path);
    let second = file_database(&path);
    let barrier = Arc::new(std::sync::Barrier::new(2));
    let run = |db: Database, barrier: Arc<std::sync::Barrier>| {
        std::thread::spawn(move || {
            barrier.wait();
            (
                db.init_default_official_providers().unwrap(),
                db.init_default_skill_repos().unwrap(),
            )
        })
    };
    let first = run(first, barrier.clone());
    let second = run(second, barrier);
    let a = first.join().unwrap();
    let b = second.join().unwrap();
    assert_eq!(a.0 + b.0, super::dao::providers_seed::OFFICIAL_SEEDS.len());
    assert_eq!(
        a.1 + b.1,
        crate::skill_model::SkillStore::default().repos.len()
    );
}

#[test]
fn universal_provider_commit_failure_does_not_poison_later_writes() {
    let db = Database::memory().unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch(
            "CREATE TABLE parent (id INTEGER PRIMARY KEY);
        CREATE TABLE child (parent_id INTEGER REFERENCES parent(id) DEFERRABLE INITIALLY DEFERRED);
        CREATE TRIGGER reject_universal_commit AFTER INSERT ON settings
        WHEN NEW.key = 'universal_providers'
        BEGIN INSERT INTO child VALUES (1); END;",
        )
        .unwrap();
    assert!(db.delete_universal_provider("missing").is_err());
    assert!(db.conn.lock().unwrap().is_autocommit());
    assert!(db.get_setting("universal_providers").unwrap().is_none());
    db.conn
        .lock()
        .unwrap()
        .execute_batch("DROP TRIGGER reject_universal_commit")
        .unwrap();
    assert!(!db.delete_universal_provider("missing").unwrap());
}

#[test]
fn log_count_cache_observes_external_connection_commits() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("shared.db");
    let db = file_database(&path);
    let other = Connection::open(&path).unwrap();
    let filters = super::usage::LogFilters::default();
    assert_eq!(db.get_request_logs(&filters, 0, 10).unwrap().total, 0);
    other.execute("INSERT INTO proxy_request_logs (request_id, provider_id, app_type, model, latency_ms, status_code, created_at)
        VALUES ('external', 'p', 'claude', 'model', 10, 200, 1000)", []).unwrap();
    let page = db.get_request_logs(&filters, 0, 10).unwrap();
    assert_eq!(page.total, 1);
    assert_eq!(page.data.len(), 1);
    other.execute("DELETE FROM proxy_request_logs", []).unwrap();
    assert_eq!(db.get_request_logs(&filters, 0, 10).unwrap().total, 0);
}

#[tokio::test]
async fn health_counter_saturates_and_rejects_corrupt_values() {
    let db = Database::memory().unwrap();
    provider(&db);
    db.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO provider_health (provider_id, app_type, consecutive_failures, updated_at)
        VALUES ('p', 'claude', ?1, 'now')",
            [u32::MAX],
        )
        .unwrap();
    db.update_provider_health_with_threshold("p", &AppType::Claude, false, None, 5)
        .await
        .unwrap();
    let health = db.get_provider_health("p", &AppType::Claude).await.unwrap();
    assert_eq!(health.consecutive_failures, u32::MAX);
    assert!(!health.is_healthy);
    for invalid in ["-1", "4294967296", "broken"] {
        db.conn
            .lock()
            .unwrap()
            .execute(
                "UPDATE provider_health SET consecutive_failures = ?1",
                [invalid],
            )
            .unwrap();
        assert!(db.get_provider_health("p", &AppType::Claude).await.is_err());
        assert!(db
            .update_provider_health_with_threshold("p", &AppType::Claude, false, None, 5)
            .await
            .is_err());
        assert!(db.conn.lock().unwrap().is_autocommit());
    }
}

#[tokio::test]
async fn circuit_breaker_numeric_limits_do_not_wrap() {
    let db = Database::memory().unwrap();
    let mut config = db.get_circuit_breaker_config().await.unwrap();
    config.failure_threshold = u32::MAX;
    config.success_threshold = u32::MAX;
    config.min_requests = u32::MAX;
    db.update_circuit_breaker_config(&config).await.unwrap();
    assert_eq!(
        db.get_circuit_breaker_config()
            .await
            .unwrap()
            .failure_threshold,
        u32::MAX
    );
    config.timeout_seconds = u64::MAX;
    assert!(db.update_circuit_breaker_config(&config).await.is_err());
    db.conn
        .lock()
        .unwrap()
        .execute("UPDATE proxy_config SET circuit_failure_threshold = -1", [])
        .unwrap();
    assert!(db.get_circuit_breaker_config().await.is_err());
}

#[test]
fn retention_rejects_negative_and_overflowing_days_without_deleting() {
    let db = Database::memory().unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO stream_check_logs
        (provider_id, provider_name, app_type, status, success, message, tested_at)
        VALUES ('p', 'Provider', 'claude', 'ok', 1, '', 0)",
            [],
        )
        .unwrap();
    for days in [-1, i64::MIN, i64::MAX] {
        assert!(db.cleanup_old_stream_check_logs(days).is_err());
    }
    assert_eq!(db.cleanup_old_stream_check_logs(7).unwrap(), 1);
}

#[test]
fn unreadable_skill_timestamp_is_reported_without_default_overwrite() {
    let db = Database::memory().unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute(
            "INSERT INTO skills (id, name, directory, updated_at)
        VALUES ('skill', 'Skill', 'skill', 'broken')",
            [],
        )
        .unwrap();
    assert!(db.get_installed_skill("skill").is_err());
    assert!(db.get_all_installed_skills().is_err());
    assert_eq!(
        db.conn
            .lock()
            .unwrap()
            .query_row("SELECT updated_at FROM skills", [], |r| r
                .get::<_, String>(0))
            .unwrap(),
        "broken"
    );
}

#[test]
fn custom_icon_mutation_keeps_unreadable_entries_intact() {
    let db = Database::memory().unwrap();
    let original = r#"[{"id":"valid","name":"Icon","data":"image"},{"id":"future","data":{"newFormat":true}}]"#;
    db.set_setting(super::CUSTOM_ICONS_KEY, original).unwrap();
    assert_eq!(db.get_custom_icons().unwrap().len(), 1);
    assert!(db
        .mutate_custom_icons(|icons| {
            icons.clear();
            Ok(())
        })
        .is_err());
    assert_eq!(
        db.get_setting(super::CUSTOM_ICONS_KEY).unwrap().as_deref(),
        Some(original)
    );
    assert!(db.conn.lock().unwrap().is_autocommit());
}

#[test]
fn usage_reads_reject_negative_and_out_of_range_persisted_counters() {
    let db = Database::memory().unwrap();
    db.conn.lock().unwrap().execute("INSERT INTO proxy_request_logs
        (request_id, provider_id, app_type, model, latency_ms, status_code, created_at, input_tokens)
        VALUES ('bad', 'p', 'claude', 'model', 10, 200, 1000, -1)", []).unwrap();
    assert!(db.get_request_detail("bad").is_err());
    assert!(db.get_usage_summary(None, None, None, None, None).is_err());
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE proxy_request_logs SET input_tokens = 4294967296",
            [],
        )
        .unwrap();
    assert!(db.get_request_detail("bad").is_err());
    db.conn
        .lock()
        .unwrap()
        .execute(
            "UPDATE proxy_request_logs SET input_tokens = 1, first_token_ms = -1",
            [],
        )
        .unwrap();
    assert!(db.get_request_detail("bad").is_err());
    assert!(db.conn.lock().unwrap().is_autocommit());
}

#[test]
fn raw_sql_import_rejects_orphans_and_missing_columns_without_replacing_live_data(
) -> Result<(), AppError> {
    let source = Database::memory()?;
    let target = Database::memory()?;
    target.set_setting("sentinel", "keep")?;
    {
        let conn = source.conn.lock().unwrap();
        conn.execute_batch("PRAGMA foreign_keys = OFF;")?;
        conn.execute("INSERT INTO provider_endpoints (provider_id, app_type, url) VALUES ('missing', 'claude', 'https://example.test')", [])?;
    }
    let sql = source.export_sql_string()?;
    assert!(target
        .import_sql_string(&sql)
        .unwrap_err()
        .to_string()
        .contains("foreign key"));
    assert_eq!(target.get_setting("sentinel")?.as_deref(), Some("keep"));
    {
        let conn = source.conn.lock().unwrap();
        conn.execute_batch(
            "DELETE FROM provider_endpoints; ALTER TABLE mcp_servers DROP COLUMN name;",
        )?;
    }
    let sql = source.export_sql_string()?;
    assert!(target
        .import_sql_string(&sql)
        .unwrap_err()
        .to_string()
        .contains("mcp_servers.name"));
    assert_eq!(target.get_setting("sentinel")?.as_deref(), Some("keep"));
    Ok(())
}
