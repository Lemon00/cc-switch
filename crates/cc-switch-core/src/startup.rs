//! 启动时依赖的后端逻辑：从 GUI crate 迁到 core，让核心单测能真实走一遍
//! 启动流程，而不是在测试里放替身。

use crate::{app::AppType, database::Database, error::AppError};

/// Initialize outbound networking identically for GUI and CLI startup.
/// Only malformed URLs are cleared; an unavailable proxy is still valid configuration.
pub fn init_outbound_http(db: &Database) -> Result<(), String> {
    use crate::proxy::http_client;
    let proxy_url = db.get_global_proxy_url().map_err(|e| e.to_string())?;
    let init_result = match proxy_url.as_deref() {
        Some(url) => match http_client::validate_proxy(Some(url)) {
            Ok(()) => http_client::init(Some(url)),
            Err(error) => {
                log::warn!("[GlobalProxy] Saved proxy URL failed validation, clearing: {error}");
                db.set_global_proxy_url(None).map_err(|e| e.to_string())?;
                http_client::init(None)
            }
        },
        None => http_client::init(None),
    };
    if let Err(error) = init_result {
        log::error!("[GlobalProxy] HTTP initialization failed (keeping saved URL): {error}");
        http_client::init(None)?;
    }
    Ok(())
}

pub const STARTUP_MCP_APPS: &[AppType] = &AppType::SUPPORTED;

pub const STARTUP_PROMPT_APPS: &[AppType] = &AppType::SUPPORTED;

/// Persist the work list before importing anything. A nonempty table may be a
/// partially completed first launch, so only the initial decision uses it.
/// An empty saved list means complete, including after deliberate user deletion.
/// Import callbacks must be idempotent: a crash can happen before progress is saved.
pub fn run_startup_imports(
    db: &Database,
    key: &str,
    apps: &[AppType],
    table_empty: impl FnOnce() -> Result<bool, AppError>,
    mut import: impl FnMut(&AppType) -> Result<usize, AppError>,
) -> Result<(), AppError> {
    let mut pending: Vec<AppType> = match db.get_setting(key)? {
        Some(value) => {
            let labels: Vec<String> = serde_json::from_str(&value)
                .map_err(|error| AppError::Config(format!("Invalid {key}: {error}")))?;
            labels
                .into_iter()
                .filter(|label| !AppType::is_retired(label))
                .map(|label| label.parse())
                .collect::<Result<Vec<_>, _>>()?
        }
        None => {
            // Existing databases without progress records keep their data as-is.
            let pending = if table_empty()? {
                apps.to_vec()
            } else {
                Vec::new()
            };
            db.set_setting(
                key,
                &serde_json::to_string(&pending)
                    .map_err(|source| AppError::JsonSerialize { source })?,
            )?;
            pending
        }
    };
    // Old progress lists may contain apps removed from this distribution.
    let previous_pending = pending.clone();
    pending.retain(AppType::is_supported);
    if pending.iter().any(|app| !apps.contains(app)) {
        return Err(AppError::Config(format!(
            "Unsupported startup import source in {key}"
        )));
    }
    let mut seen = std::collections::HashSet::new();
    pending.retain(|app| seen.insert(app.clone()));
    let normalized_pending =
        serde_json::to_string(&pending).map_err(|source| AppError::JsonSerialize { source })?;
    if pending != previous_pending
        || db.get_setting(key)?.as_deref() != Some(normalized_pending.as_str())
    {
        db.set_setting(
            key,
            &serde_json::to_string(&pending)
                .map_err(|source| AppError::JsonSerialize { source })?,
        )?;
    }
    let mut failures = Vec::new();
    for app in pending.clone() {
        match import(&app) {
            Ok(count) => {
                pending.retain(|remaining| remaining != &app);
                // Stop if progress cannot be persisted; retain the on-disk retry list.
                db.set_setting(
                    key,
                    &serde_json::to_string(&pending)
                        .map_err(|source| AppError::JsonSerialize { source })?,
                )?;
                log::debug!("Startup import {key}/{} completed ({count})", app.as_str());
            }
            // This aggregate is logged by startup callers. TOML diagnostics
            // include the original line, which may contain a credential.
            Err(error) => failures.push(format!(
                "{}: {}",
                app.as_str(),
                crate::error_for_log(&error.to_string())
            )),
        }
    }
    if failures.is_empty() {
        Ok(())
    } else {
        Err(AppError::Message(format!(
            "Startup import {key} incomplete; will retry: {}",
            failures.join("; ")
        )))
    }
}

pub fn initialize_common_config_snippets(state: &crate::store::AppState) {
    // Auto-extract common config snippets from clean live files when snippet is missing.
    // This must run before proxy mode is re-attached on startup, otherwise we'd read
    // proxy-placeholder configs instead of the user's actual live settings. A client
    // still attached from an update restart (no detach on the way out) is skipped too.
    for app_type in crate::app_config::AppType::all() {
        if !state
            .db
            .should_auto_extract_config_snippet(app_type.as_str())
            .unwrap_or(false)
        {
            continue;
        }
        if state.proxy_service.live_has_proxy_placeholder(&app_type) {
            continue;
        }

        let settings = match crate::services::provider::ProviderService::read_live_settings(
            app_type.clone(),
        ) {
            Ok(s) => s,
            Err(_) => continue,
        };

        match crate::services::provider::ProviderService::extract_common_config_snippet_from_settings(
            app_type.clone(),
            &settings,
        ) {
            Ok(snippet) if !snippet.is_empty() && snippet != "{}" => {
                match state.db.set_config_snippet(app_type.as_str(), Some(snippet)) {
                    Ok(()) => {
                        let _ = state.db.set_config_snippet_cleared(app_type.as_str(), false);
                        log::info!(
                            "✓ Auto-extracted common config snippet for {}",
                            app_type.as_str()
                        );
                    }
                    Err(e) => log::warn!(
                        "✗ Failed to save config snippet for {}: {}",
                        app_type.as_str(),
                        crate::error_for_log(&e.to_string())
                    ),
                }
            }
            Ok(_) => log::debug!(
                "○ Live config for {} has no extractable common fields",
                app_type.as_str()
            ),
            Err(e) => log::warn!(
                "✗ Failed to extract config snippet for {}: {}",
                app_type.as_str(),
                crate::error_for_log(&e.to_string())
            ),
        }
    }

    // 读标志失败时 fail-open 再跑一遍（宁可重复迁移，也不要永久跳过）。
    let should_run_legacy_migration = state
        .db
        .is_legacy_common_config_migrated()
        .map(|done| !done)
        .unwrap_or(true);

    if should_run_legacy_migration {
        let mut all_ok = true;
        for app_type in [
            crate::app_config::AppType::Claude,
            crate::app_config::AppType::Codex,
        ] {
            if let Err(e) = crate::services::provider::ProviderService::migrate_legacy_common_config_usage_if_needed(
                state,
                app_type.clone(),
            ) {
                all_ok = false;
                log::warn!(
                    "✗ Failed to migrate legacy common-config usage for {}: {}",
                    app_type.as_str(),
                    crate::error_for_log(&e.to_string())
                );
            }
        }

        // 任一步失败都不打完成标记，下次启动继续重试。
        if all_ok {
            if let Err(e) = state.db.set_legacy_common_config_migrated(true) {
                log::warn!("✗ Failed to persist legacy common-config migration flag: {e}");
            }
        } else {
            log::warn!("✗ Legacy common-config migration incomplete; will retry on next startup");
        }
    }
}

/// Application bootstrap selects sync policy and imports the local pricing file.
/// Database constructors themselves only initialize persistence.
pub fn open_database() -> Result<crate::database::Database, crate::error::AppError> {
    let db = crate::database::Database::init_with_observer(std::sync::Arc::new(
        crate::services::auto_sync::AutoSyncDatabaseObserver,
    ))?;
    if let Err(error) = crate::services::model_pricing::sync_local_model_pricing(&db) {
        log::warn!("Failed to sync local model pricing file: {error}");
    }
    Ok(db)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[serial_test::serial]
    fn outbound_startup_loads_saved_proxy_and_preserves_unreachable_valid_urls() {
        use crate::proxy::http_client;
        let saved = http_client::get_current_proxy_url();
        let db = Database::memory().unwrap();
        let url = "http://127.0.0.1:1";
        db.set_global_proxy_url(Some(url)).unwrap();
        init_outbound_http(&db).unwrap();
        assert_eq!(http_client::get_current_proxy_url().as_deref(), Some(url));
        assert_eq!(db.get_global_proxy_url().unwrap().as_deref(), Some(url));
        db.set_global_proxy_url(Some("invalid://proxy")).unwrap();
        init_outbound_http(&db).unwrap();
        assert!(db.get_global_proxy_url().unwrap().is_none());
        assert!(http_client::get_current_proxy_url().is_none());
        http_client::init(saved.as_deref()).unwrap();
    }

    const KEY: &str = "startup_import_test";
    const APPS: &[AppType] = &[AppType::Claude, AppType::Codex];

    #[test]
    fn startup_import_diagnostics_omit_config_source_and_preserve_retry_progress() {
        let db = Database::memory().unwrap();
        let secret = "sk-startup-regression-secret";
        let malformed = format!("experimental_bearer_token = \"{secret}\" !\n");
        let error = run_startup_imports(
            &db,
            KEY,
            APPS,
            || Ok(true),
            |app| {
                if app == &AppType::Codex {
                    crate::codex_config::validate_config_toml(&malformed)?;
                }
                Ok(0)
            },
        )
        .unwrap_err()
        .to_string();
        assert!(!error.contains(secret), "{error}");
        assert!(error.contains("line 1"), "{error}");
        assert!(error.contains("codex"), "{error}");
        assert_eq!(db.get_setting(KEY).unwrap().as_deref(), Some("[\"codex\"]"));
    }

    #[test]
    fn startup_import_retries_only_failed_sources_even_after_table_is_nonempty() {
        let db = Database::memory().unwrap();
        let import = |app: &AppType| -> Result<usize, AppError> {
            db.save_prompt(
                app,
                &crate::prompt::Prompt {
                    id: "startup".into(),
                    name: "Imported".into(),
                    content: "native prompt".into(),
                    description: None,
                    enabled: true,
                    created_at: None,
                    updated_at: None,
                },
            )?;
            Ok(1)
        };
        let first = run_startup_imports(
            &db,
            KEY,
            APPS,
            || db.is_prompts_table_empty(),
            |app| {
                if app == &AppType::Codex {
                    Err(AppError::Message("unreadable file".into()))
                } else {
                    import(app)
                }
            },
        );
        assert!(first.is_err());
        assert!(!db.is_prompts_table_empty().unwrap());
        assert_eq!(db.get_setting(KEY).unwrap().as_deref(), Some("[\"codex\"]"));
        let mut retried = Vec::new();
        run_startup_imports(
            &db,
            KEY,
            APPS,
            || panic!("must use saved progress"),
            |app| {
                retried.push(app.clone());
                import(app)
            },
        )
        .unwrap();
        assert_eq!(retried, [AppType::Codex]);
        assert_eq!(db.get_prompts(&AppType::Codex).unwrap().len(), 1);
        run_startup_imports(&db, KEY, APPS, || Ok(true), |_| panic!("already complete")).unwrap();
    }

    #[test]
    fn startup_import_preserves_existing_databases_without_progress() {
        let db = Database::memory().unwrap();
        run_startup_imports(&db, KEY, APPS, || Ok(false), |_| panic!("existing data")).unwrap();
        // A subsequent deliberate deletion must not re-enable first-launch import.
        run_startup_imports(&db, KEY, APPS, || Ok(true), |_| panic!("user deleted data")).unwrap();
        assert_eq!(db.get_setting(KEY).unwrap().as_deref(), Some("[]"));
    }

    #[test]
    fn startup_import_requires_durable_progress_before_side_effects() {
        let db = Database::memory().unwrap();
        db.conn
            .lock()
            .unwrap()
            .execute_batch("PRAGMA query_only = ON")
            .unwrap();
        assert!(run_startup_imports(
            &db,
            KEY,
            APPS,
            || Ok(true),
            |_| panic!("progress was not saved")
        )
        .is_err());
    }

    #[test]
    fn startup_import_rejects_invalid_progress_without_importing() {
        let db = Database::memory().unwrap();
        for value in [
            "invalid",
            "[\"unknown-app\"]",
            "[\"dsh\"]",
            "[\"gemini\",\"dsh\"]",
        ] {
            db.set_setting(KEY, value).unwrap();
            assert!(run_startup_imports(
                &db,
                KEY,
                APPS,
                || Ok(true),
                |_| panic!("invalid progress")
            )
            .is_err());
            assert_eq!(db.get_setting(KEY).unwrap().as_deref(), Some(value));
        }
    }

    #[test]
    fn startup_import_drops_retired_progress_without_touching_native_files() {
        let db = Database::memory().unwrap();
        db.set_setting(KEY, "[\"gemini\",\"pi\",\"claude\",\"claude\"]")
            .unwrap();
        let mut imported = Vec::new();
        run_startup_imports(
            &db,
            KEY,
            STARTUP_MCP_APPS,
            || Ok(false),
            |app| {
                imported.push(app.clone());
                Ok(1)
            },
        )
        .unwrap();
        assert_eq!(imported, vec![AppType::Claude]);
        assert_eq!(db.get_setting(KEY).unwrap().as_deref(), Some("[]"));
        db.set_setting(KEY, "[\"openclaw\"]").unwrap();
        run_startup_imports(
            &db,
            KEY,
            STARTUP_MCP_APPS,
            || Ok(false),
            |_| panic!("retired app imported"),
        )
        .unwrap();
        assert_eq!(db.get_setting(KEY).unwrap().as_deref(), Some("[]"));
    }

    #[test]
    fn startup_mcp_sources_cover_all_supported_clients() {
        let supported: std::collections::HashSet<_> =
            AppType::all().filter(AppType::supports_mcp).collect();
        assert_eq!(
            STARTUP_MCP_APPS
                .iter()
                .cloned()
                .collect::<std::collections::HashSet<_>>(),
            supported
        );
    }
}
