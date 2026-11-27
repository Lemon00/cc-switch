use serde_json::{json, Value};

use crate::error::AppError;
use crate::services::{model_pricing, PromptService, ProviderService};
use crate::settings;
use crate::store::AppState;

pub(crate) fn run_post_import_sync(app_state: &AppState) -> Result<(), AppError> {
    let mut failures = Vec::new();

    if let Err(error) = super::global_proxy::reload_proxy_from_database(&app_state.db) {
        failures.push(format!("outbound proxy: {error}"));
    }

    if let Err(error) = ProviderService::sync_current_to_live(app_state) {
        failures.push(format!("live configuration: {error}"));
    }
    if let Err(error) = PromptService::sync_all_to_live(app_state) {
        failures.push(format!("prompts: {error}"));
    }
    if let Err(error) = model_pricing::sync_local_model_pricing(&app_state.db) {
        failures.push(format!("model pricing: {error}"));
    }
    if let Err(error) = settings::reload_settings() {
        failures.push(format!("settings cache: {error}"));
    }

    match app_state.db.get_log_config() {
        Ok(log_config) => log::set_max_level(log_config.to_level_filter()),
        Err(error) => {
            log::set_max_level(log::LevelFilter::Info);
            failures.push(format!("runtime log level: {error}"));
        }
    }
    app_state.usage_cache.invalidate_all();

    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::Message(format!(
            "部分导入后同步失败: {}",
            failures.join("; ")
        )))
    }
}

fn post_sync_warning<E: std::fmt::Display>(err: E) -> String {
    AppError::localized(
        "sync.post_operation_sync_failed",
        format!("后置同步状态失败: {err}"),
        format!("Post-operation synchronization failed: {err}"),
    )
    .to_string()
}

pub(crate) fn post_sync_warning_from_result(
    result: Result<Result<(), AppError>, String>,
) -> Option<String> {
    match result {
        Ok(Ok(())) => None,
        Ok(Err(err)) => Some(post_sync_warning(err)),
        Err(err) => Some(post_sync_warning(err)),
    }
}

pub(crate) fn attach_warning(mut value: Value, warning: Option<String>) -> Value {
    if let Some(message) = warning {
        if let Some(obj) = value.as_object_mut() {
            obj.insert("warning".to_string(), Value::String(message));
        }
    }
    value
}

pub(crate) fn success_payload_with_warning(backup_id: String, warning: Option<String>) -> Value {
    attach_warning(
        json!({
            "success": true,
            "message": "SQL imported successfully",
            "backupId": backup_id
        }),
        warning,
    )
}

#[cfg(test)]
mod tests {
    use super::{attach_warning, post_sync_warning_from_result};
    use serde_json::json;

    #[test]
    #[serial_test::serial]
    fn restored_database_refreshes_outbound_clients_even_if_live_projection_fails() {
        use crate::database::Database;
        use crate::proxy::http_client;
        use crate::store::AppState;
        use std::sync::Arc;

        let _home = crate::commands::startup_tests::TestHome::new();
        struct RestoreProxy(Option<String>);
        impl Drop for RestoreProxy {
            fn drop(&mut self) {
                let _ = http_client::apply_proxy(self.0.as_deref());
            }
        }
        let _restore = RestoreProxy(http_client::get_current_proxy_url());
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        let saved = Database::memory().unwrap();
        let provider = crate::provider::Provider::with_id(
            "restored".into(),
            "Restored".into(),
            json!({"env": {"ANTHROPIC_API_KEY": "fixture"}}),
            None,
        );
        saved
            .save_provider(&crate::app_config::AppType::Claude, &provider)
            .unwrap();
        saved
            .set_current_provider(&crate::app_config::AppType::Claude, &provider.id)
            .unwrap();
        // A malformed native file must not prevent other restored state from
        // becoming effective or turn a partial restore into silent success.
        let path = crate::config::get_claude_settings_path();
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "broken json").unwrap();
        for proxy in [Some("http://127.0.0.1:28182"), None] {
            http_client::apply_proxy(Some("http://127.0.0.1:28181")).unwrap();
            saved.set_global_proxy_url(proxy).unwrap();
            state
                .db
                .import_sql_string(&saved.export_sql_string().unwrap())
                .unwrap();
            let error = super::run_post_import_sync(&state).unwrap_err().to_string();
            assert!(error.contains("live configuration"), "{error}");
            assert_eq!(state.db.get_global_proxy_url().unwrap().as_deref(), proxy);
            assert_eq!(http_client::get_current_proxy_url().as_deref(), proxy);
        }
    }

    #[test]
    #[serial_test::serial]
    fn invalid_restored_proxy_is_reported_without_losing_the_saved_value() {
        let _home = crate::commands::startup_tests::TestHome::new();
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        state
            .db
            .set_global_proxy_url(Some("ftp://invalid.example"))
            .unwrap();
        let error = super::run_post_import_sync(&state).unwrap_err().to_string();
        assert!(error.contains("outbound proxy"), "{error}");
        assert_eq!(
            state.db.get_global_proxy_url().unwrap().as_deref(),
            Some("ftp://invalid.example")
        );
    }

    #[test]
    fn post_sync_warning_from_result_returns_none_on_success() {
        let warning = post_sync_warning_from_result(Ok(Ok(())));
        assert!(warning.is_none());
    }

    #[test]
    fn post_sync_warning_from_result_returns_some_on_sync_error() {
        let warning =
            post_sync_warning_from_result(Ok(Err(crate::error::AppError::Config("boom".into()))));
        assert!(warning.is_some());
    }

    #[tokio::test]
    async fn post_sync_warning_from_result_returns_some_on_join_error() {
        let handle = tokio::spawn(async move {
            panic!("forced join error");
        });
        let join_err = handle.await.expect_err("task should panic");
        let warning = post_sync_warning_from_result(Err(join_err.to_string()));
        assert!(warning.is_some());
    }

    #[test]
    fn attach_warning_adds_warning_without_dropping_existing_fields() {
        let payload = json!({ "status": "downloaded" });
        let updated = attach_warning(payload, Some("post sync warning".to_string()));
        assert_eq!(
            updated.get("status").and_then(|v| v.as_str()),
            Some("downloaded")
        );
        assert_eq!(
            updated.get("warning").and_then(|v| v.as_str()),
            Some("post sync warning")
        );
    }
}
