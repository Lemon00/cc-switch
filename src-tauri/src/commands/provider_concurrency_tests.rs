use std::sync::Arc;
use std::time::Duration;

use tauri::Manager;

use super::startup_tests::TestHome;
use crate::app_config::AppType;
use crate::database::Database;
use crate::provider::Provider;
use crate::store::AppState;

#[test]
#[serial_test::serial]
fn native_provider_refresh_waits_for_mutations_while_catalog_reads_stay_available() {
    let _home = TestHome::new();
    let db = Arc::new(Database::memory().unwrap());
    let app = tauri::test::mock_builder()
        .manage(AppState::new(db.clone()))
        .build(tauri::test::mock_context(tauri::test::noop_assets()))
        .unwrap();

    for app_type in ["dsh"] {
        let provider = Provider::with_id(
            "deleted".into(),
            "Deleted".into(),
            serde_json::json!({}),
            None,
        );
        let app_enum = app_type.parse::<AppType>().unwrap();
        db.save_provider(&app_enum, &provider).unwrap();
        let state = app.state::<AppState>();
        let guard = tauri::async_runtime::block_on(state.operations.lock_for_app(app_type));
        let mut listing = std::pin::pin!(super::refresh_providers(app.state(), app_type.into()));
        let early_result = tauri::async_runtime::block_on(async {
            // Catalog reads are read-only and may return the last committed snapshot.
            let snapshot = tokio::time::timeout(
                Duration::from_secs(5),
                super::get_providers(app.state(), app_type.into()),
            )
            .await
            .unwrap()
            .unwrap();
            assert!(snapshot.contains_key(&provider.id));
            // A mutation for this app must not stall unrelated provider reads.
            tokio::time::timeout(
                Duration::from_secs(5),
                super::get_providers(app.state(), "opencode".into()),
            )
            .await
            .unwrap()
            .unwrap();
            tokio::time::timeout(Duration::from_millis(200), listing.as_mut()).await
        });
        // Complete the in-flight mutation while still owning the app lock.
        db.delete_provider(&app_enum, &provider.id).unwrap();
        drop(guard);
        let waited = early_result.is_err();
        let listed = match early_result {
            Ok(result) => result,
            Err(_) => tauri::async_runtime::block_on(async {
                tokio::time::timeout(Duration::from_secs(5), listing)
                    .await
                    .unwrap()
            }),
        }
        .unwrap();
        assert!(waited, "{app_type} refreshed an unfinished mutation");
        assert!(!listed.contains_key(&provider.id));
        assert!(db
            .get_provider_by_id(&provider.id, &app_enum)
            .unwrap()
            .is_none());
    }
}
