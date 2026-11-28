//! Provider service module
//!
//! Handles provider CRUD operations, switching, and configuration management.

pub mod agy_direct;
mod catalog;
pub mod claude_direct;
pub mod claude_editor;
pub mod claude_official;
pub mod codex_client_catalog;
pub mod codex_direct;
pub mod codex_editor;
pub mod codex_login;
pub mod codex_oauth_switch;
pub mod codex_official_models;
pub mod codex_terminal;
pub mod editor_toml;
pub mod endpoints;
pub mod icon;
pub mod live;
#[cfg(test)]
pub mod opencode_tests;
pub mod usage;

use indexmap::IndexMap;
use serde::Deserialize;
use serde_json::Value;

use crate::app_config::AppType;
use crate::database::UnreadableEntry;
use crate::error::AppError;
use crate::provider::{Provider, ProviderMeta, UsageResult};
use crate::services::mcp::McpService;
use crate::settings::CustomEndpoint;
use crate::store::AppState;

// Re-export sub-module functions for external access
pub use live::{
    import_default_config, import_opencode_providers_from_live, read_live_settings,
    should_import_default_config_on_startup, sync_current_to_live,
};

pub use claude_editor::{switch_drops as claude_switch_drops, SwitchDrops};
pub use claude_editor::{EditorSave, EditorView};

// Internal re-exports (pub)
pub use live::{
    initialize_dsh_profiles, provider_exists_in_live_config, sync_additive_app_to_live,
    write_live_for_state, LiveSyncOutcome,
};

// Internal re-exports
use live::remove_opencode_provider_from_live;
use usage::validate_usage_script;

pub fn official_provider_supports_proxy_takeover(app_type: &AppType, provider: &Provider) -> bool {
    matches!(app_type, AppType::Codex)
        && crate::proxy::providers::is_codex_official_provider(provider)
}

/// 统一会话开关变更后，立即按新开关状态重写当前官方 Codex 供应商的选路（关键字段），
/// 使开关即时生效，无需等下一次切换。当前供应商非官方（或不存在）时为 no-op：开关只
/// 影响官方直连的选路。代理模式下 live 是代理契约，不受这个开关影响。
pub fn reapply_current_codex_official_live(state: &AppState) -> Result<bool, AppError> {
    let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &AppType::Codex)?;
    let current_id = ProviderService::current(state, AppType::Codex)?;
    if current_id.is_empty() {
        return Ok(false);
    }
    let providers = state.db.get_all_providers(&AppType::Codex)?;
    let Some(provider) = providers.get(&current_id) else {
        return Ok(false);
    };
    if !codex_direct::is_official(provider) {
        return Ok(false);
    }
    live::sync_live_for_provider_respecting_mode(state, &AppType::Codex, provider, None)?;
    Ok(true)
}

/// 「非接管切换时保留官方登录」开关变更后，立即按新开关重写当前直连的第三方 Codex 供应商：
/// 关掉时把 `auth.json` 里的官方登录暂存走、`requires_openai_auth` 改写成 false，和重新切换
/// 一次一样，不必先切回官方卡再切回来（#9367）。开关只管直连切到第三方：代理模式、当前是
/// 官方卡或没有当前供应商时为 no-op。
pub fn reapply_current_codex_third_party_live(state: &AppState) -> Result<bool, AppError> {
    let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &AppType::Codex)?;
    if crate::mode::current::is_proxy(&AppType::Codex) {
        return Ok(false);
    }
    let Some(current_id) = crate::mode::current::provider_for(
        &state.db,
        &AppType::Codex,
        crate::mode::current::Purpose::Direct,
    )?
    else {
        return Ok(false);
    };
    let providers = state.db.get_all_providers(&AppType::Codex)?;
    let Some(provider) = providers.get(&current_id) else {
        return Ok(false);
    };
    if codex_direct::is_official(provider) {
        return Ok(false);
    }
    live::write_live_for_state(state, &AppType::Codex, provider)?;
    Ok(true)
}

/// 新版不再读通用配置片段，但旧设备经云同步拿到新建的行时仍按这个标记合并片段；不写的
/// 话，旧版切到它会把 hooks、MCP 等共享设置整份抹掉。
fn keep_common_config_for_old_versions(provider: &mut Provider) {
    provider
        .meta
        .get_or_insert_with(Default::default)
        .common_config_enabled = Some(true);
}

/// 编辑器保存的是新增的供应商，还是已有的。
#[derive(Clone, Copy, PartialEq, Eq)]
enum EditorSaveKind {
    Add,
    Update,
}

impl EditorSaveKind {
    fn validate_target(self, state: &AppState, app: &AppType, id: &str) -> Result<(), AppError> {
        if id.trim().is_empty() {
            return Err(AppError::InvalidInput(
                "Provider ID must not be empty".into(),
            ));
        }
        let exists = state.db.provider_exists(id, app)?;
        match (self, exists) {
            (Self::Add, true) => Err(AppError::InvalidInput(format!(
                "Provider '{id}' already exists in app '{}'",
                app.as_str()
            ))),
            (Self::Update, false) => Err(AppError::InvalidInput(format!(
                "Provider '{id}' does not exist in app '{}'",
                app.as_str()
            ))),
            _ => Ok(()),
        }
    }

    /// 这次保存要不要把关键字段也换进 live：直连模式下编辑的是当前供应商，或者新增的是
    /// 第一个供应商。代理模式下 live 的关键字段是代理契约，只写全局改动。
    fn writes_key_fields(
        self,
        state: &AppState,
        app_type: &AppType,
        mode: &crate::mode::state::ModeState,
        id: &str,
    ) -> Result<bool, AppError> {
        if mode.is_proxy() {
            return Ok(false);
        }
        let direct = crate::mode::current::provider_for(
            &state.db,
            app_type,
            crate::mode::current::Purpose::Direct,
        )?;
        Ok(match self {
            Self::Add => direct.is_none(),
            Self::Update => direct.as_deref() == Some(id),
        })
    }
}

/// Provider business logic service
pub struct ProviderService;

/// Native reconciliation refreshes saved rows. Keep the
/// entire snapshot/readback under the same per-app lock as provider mutations,
/// so an in-flight list cannot restore a deleted row or overwrite a newer edit.
fn lock_native_provider_operations(
    state: &AppState,
    app_type: &AppType,
) -> Option<tokio::sync::OwnedMutexGuard<()>> {
    matches!(app_type, AppType::Dsh | AppType::OpenCode | AppType::Agy)
        .then(|| futures::executor::block_on(state.operations.lock_for_app(app_type.as_str())))
}

/// Result of a provider switch operation, including any non-fatal warnings
#[derive(Debug, serde::Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct SwitchResult {
    pub warnings: Vec<String>,
    /// Present only when this operation selected a local proxy target.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub route: Option<crate::mode::controller::RouteSwitchResult>,
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    use crate::config::{get_claude_settings_path, read_json_file, write_json_file};
    use crate::database::Database;
    use crate::provider::{
        AuthBinding, AuthBindingSource, ClaudeModelConfig, ProviderMeta, UniversalProvider,
        UsageScript,
    };
    #[cfg(any(target_os = "macos", windows, target_os = "linux"))]
    use crate::proxy::types::ProxyConfig;
    use crate::store::AppState;
    use serde_json::json;
    use serial_test::serial;
    use std::env;
    use std::fs;
    use std::path::{Path, PathBuf};
    use std::sync::{Arc, Mutex, OnceLock};
    use tempfile::TempDir;

    struct TempHome {
        #[allow(dead_code)]
        dir: TempDir,
        original_home: Option<String>,
        #[cfg(windows)]
        original_local_app_data: Option<String>,
        original_userprofile: Option<String>,
        original_test_home: Option<String>,
        #[cfg(target_os = "linux")]
        original_xdg_config_home: Option<std::ffi::OsString>,
    }

    impl TempHome {
        fn new() -> Self {
            let dir = TempDir::new().expect("failed to create temp home");
            let original_home = env::var("HOME").ok();
            #[cfg(windows)]
            let original_local_app_data = env::var("LOCALAPPDATA").ok();
            let original_userprofile = env::var("USERPROFILE").ok();
            let original_test_home = env::var("CC_SWITCH_TEST_HOME").ok();
            #[cfg(target_os = "linux")]
            let original_xdg_config_home = env::var_os("XDG_CONFIG_HOME");

            env::set_var("HOME", dir.path());
            #[cfg(windows)]
            env::set_var("LOCALAPPDATA", dir.path().join("AppData").join("Local"));
            env::set_var("USERPROFILE", dir.path());
            env::set_var("CC_SWITCH_TEST_HOME", dir.path());
            // Claude Desktop Linux paths follow XDG_CONFIG_HOME; pin them under the temp home.
            #[cfg(target_os = "linux")]
            env::remove_var("XDG_CONFIG_HOME");
            crate::settings::update_settings(Default::default())
                .expect("initialize settings for isolated test home");

            Self {
                dir,
                original_home,
                #[cfg(windows)]
                original_local_app_data,
                original_userprofile,
                original_test_home,
                #[cfg(target_os = "linux")]
                original_xdg_config_home,
            }
        }
    }

    impl Drop for TempHome {
        fn drop(&mut self) {
            match &self.original_home {
                Some(value) => env::set_var("HOME", value),
                None => env::remove_var("HOME"),
            }

            #[cfg(windows)]
            {
                match &self.original_local_app_data {
                    Some(value) => env::set_var("LOCALAPPDATA", value),
                    None => env::remove_var("LOCALAPPDATA"),
                }
            }

            match &self.original_userprofile {
                Some(value) => env::set_var("USERPROFILE", value),
                None => env::remove_var("USERPROFILE"),
            }

            match &self.original_test_home {
                Some(value) => env::set_var("CC_SWITCH_TEST_HOME", value),
                None => env::remove_var("CC_SWITCH_TEST_HOME"),
            }

            #[cfg(target_os = "linux")]
            {
                match &self.original_xdg_config_home {
                    Some(value) => env::set_var("XDG_CONFIG_HOME", value),
                    None => env::remove_var("XDG_CONFIG_HOME"),
                }
            }
        }
    }

    fn test_guard() -> std::sync::MutexGuard<'static, ()> {
        static LOCK: OnceLock<Mutex<()>> = OnceLock::new();
        LOCK.get_or_init(|| Mutex::new(()))
            .lock()
            .unwrap_or_else(|err| err.into_inner())
    }

    fn with_test_home<T>(test: impl FnOnce(&AppState, &Path) -> T) -> T {
        let _guard = test_guard();
        let temp = TempHome::new();
        let db = Arc::new(Database::memory().expect("in-memory database"));
        let state = AppState::new(db);
        test(&state, temp.dir.path())
    }

    fn codex_settings(base_url: &str, api_key: &str) -> Value {
        json!({
            "auth": {
                "OPENAI_API_KEY": api_key
            },
            "config": format!(
                "model_provider = \"custom\"\n\
                 [model_providers.custom]\n\
                 name = \"custom\"\n\
                 base_url = \"{base_url}\"\n\
                 wire_api = \"chat\"\n"
            )
        })
    }

    #[test]
    #[serial]
    fn first_claude_add_rolls_back_before_live_publication_and_can_retry() {
        with_test_home(|state, _| {
            let path = get_claude_settings_path();
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(&path, "{broken configuration").unwrap();
            let provider = Provider::with_id(
                "first-claude".into(),
                "First".into(),
                json!({"env": {"ANTHROPIC_AUTH_TOKEN": "fixture"}}),
                None,
            );
            assert!(ProviderService::add(state, AppType::Claude, provider.clone(), true).is_err());
            assert!(!state
                .db
                .provider_exists(&provider.id, &AppType::Claude)
                .unwrap());
            assert_eq!(fs::read_to_string(&path).unwrap(), "{broken configuration");
            assert!(state
                .db
                .get_current_provider(&AppType::Claude)
                .unwrap()
                .is_none());
            fs::write(&path, "{}").unwrap();
            assert!(ProviderService::add(state, AppType::Claude, provider.clone(), true).unwrap());
            assert_eq!(
                ProviderService::current(state, AppType::Claude).unwrap(),
                provider.id
            );
        });
    }

    #[test]
    #[serial]
    fn first_claude_add_keeps_published_row_for_recovery() {
        with_test_home(|state, _| {
            state
                .db
                .conn
                .lock()
                .unwrap()
                .execute_batch(
                    "CREATE TRIGGER reject_first_claude_current
                 BEFORE UPDATE OF is_current ON providers
                 WHEN NEW.app_type = 'claude' AND NEW.is_current = 1
                 BEGIN SELECT RAISE(ABORT, 'forced current failure'); END;",
                )
                .unwrap();
            let provider = Provider::with_id(
                "first-claude".into(),
                "First".into(),
                json!({"env": {"ANTHROPIC_AUTH_TOKEN": "fixture"}}),
                None,
            );
            assert!(ProviderService::add(state, AppType::Claude, provider.clone(), true).is_err());
            assert!(state
                .db
                .provider_exists(&provider.id, &AppType::Claude)
                .unwrap());
            assert!(crate::mode::operation::has_pending("claude"));
            let live: Value = read_json_file(&get_claude_settings_path()).unwrap();
            assert_eq!(live["env"]["ANTHROPIC_AUTH_TOKEN"], "fixture");
            state
                .db
                .conn
                .lock()
                .unwrap()
                .execute_batch("DROP TRIGGER reject_first_claude_current;")
                .unwrap();
            crate::mode::operation::recover_on_startup(&state.db);
            assert!(!crate::mode::operation::has_pending("claude"));
            assert_eq!(
                ProviderService::current(state, AppType::Claude).unwrap(),
                provider.id
            );
        });
    }

    #[test]
    #[serial]
    fn concurrent_provider_adds_keep_the_first_row() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().unwrap();
            let barrier = std::sync::Barrier::new(2);
            let results = std::thread::scope(|scope| {
                let add = |name: &str| {
                    barrier.wait();
                    ProviderService::add_without_switching(
                        state,
                        AppType::Claude,
                        Provider::with_id("same-id".into(), name.into(), json!({}), None),
                    )
                };
                let first = scope.spawn(move || add("First"));
                let second = scope.spawn(move || add("Second"));
                [first.join().unwrap(), second.join().unwrap()]
            });
            assert_eq!(results.iter().filter(|r| r.is_ok()).count(), 1);
            assert_eq!(results.iter().filter(|r| r.is_err()).count(), 1);
        });
    }

    #[test]
    #[serial]
    fn profile_resnapshot_preserves_unknown_fields_and_clear_saves_changes() {
        with_test_home(|state, _| {
            use crate::services::profile::{ProfileScope, ProfileService};
            crate::settings::reload_settings().unwrap();
            let mut profile =
                ProfileService::create(state, "Project", ProfileScope::Claude).unwrap();
            let mut payload: Value = serde_json::from_str(&profile.payload).unwrap();
            payload["future"] = json!({"keep": true});
            payload["providers"]["future-app"] = json!("future-provider");
            payload["providers"]["codex"] = json!("codex-provider");
            profile.payload = payload.to_string();
            state.db.save_profile(&profile).unwrap();
            state
                .db
                .set_current_profile_id("claude", Some(&profile.id))
                .unwrap();
            let provider =
                Provider::with_id("profile-provider".into(), "P".into(), json!({}), None);
            state.db.save_provider(&AppType::Claude, &provider).unwrap();
            state
                .db
                .set_current_provider(&AppType::Claude, &provider.id)
                .unwrap();
            ProfileService::clear_current(state, ProfileScope::Claude).unwrap();
            let saved = state.db.get_profile(&profile.id).unwrap().unwrap();
            let saved: Value = serde_json::from_str(&saved.payload).unwrap();
            assert_eq!(saved["future"], payload["future"]);
            assert_eq!(saved["providers"]["future-app"], "future-provider");
            assert_eq!(saved["providers"]["codex"], "codex-provider");
            assert_eq!(saved["providers"]["claude"], "profile-provider");
            assert_eq!(state.db.get_current_profile_id("claude").unwrap(), None);
        });
    }

    #[test]
    #[serial]
    fn profile_invalid_target_and_repeated_apply_do_not_mutate_current_snapshot() {
        with_test_home(|state, _| {
            use crate::services::profile::{ProfileScope, ProfileService};
            crate::settings::reload_settings().unwrap();
            let profile = ProfileService::create(state, "Project", ProfileScope::Claude).unwrap();
            state
                .db
                .set_current_profile_id("claude", Some(&profile.id))
                .unwrap();
            let provider = Provider::with_id("new-current".into(), "P".into(), json!({}), None);
            state.db.save_provider(&AppType::Claude, &provider).unwrap();
            state
                .db
                .set_current_provider(&AppType::Claude, &provider.id)
                .unwrap();
            assert!(ProfileService::apply(state, "missing", ProfileScope::Claude).is_err());
            let mut broken = profile.clone();
            broken.id = "broken".into();
            broken.payload = "{".into();
            state.db.save_profile(&broken).unwrap();
            assert!(ProfileService::apply(state, "broken", ProfileScope::Claude).is_err());
            assert!(
                ProfileService::apply(state, &profile.id, ProfileScope::Claude)
                    .unwrap()
                    .is_empty()
            );
            assert_eq!(
                state.db.get_profile(&profile.id).unwrap().unwrap().payload,
                profile.payload
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Claude)
                    .unwrap()
                    .as_deref(),
                Some("new-current")
            );
        });
    }

    #[test]
    #[serial]
    fn universal_sync_rejects_in_use_disabled_child_before_updating_siblings() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().unwrap();
            let mut parent = UniversalProvider::new(
                "preflight".into(),
                "Original".into(),
                "newapi".into(),
                "https://example.test".into(),
                "key".into(),
            );
            parent.apps.claude = true;
            parent.apps.codex = true;
            state.db.save_universal_provider(&parent).unwrap();
            ProviderService::sync_universal_to_apps(state, &parent.id).unwrap();
            state
                .db
                .set_current_provider(&AppType::Codex, "universal-codex-preflight")
                .unwrap();
            parent.apps.codex = false;
            parent.name = "Changed".into();
            state.db.save_universal_provider(&parent).unwrap();
            assert!(ProviderService::sync_universal_to_apps(state, &parent.id).is_err());
            assert_eq!(
                state
                    .db
                    .get_provider_by_id("universal-claude-preflight", &AppType::Claude)
                    .unwrap()
                    .unwrap()
                    .name,
                "Original"
            );
        });
    }

    #[test]
    #[serial]
    fn deleting_stack_member_completes_without_relocking_application() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().unwrap();
            let provider = Provider::with_id(
                "stack-delete".into(),
                "Stack".into(),
                json!({"env": {"ANTHROPIC_BASE_URL": "https://example.test", "ANTHROPIC_AUTH_TOKEN": "key"}}),
                None,
            );
            state.db.save_provider(&AppType::Claude, &provider).unwrap();
            crate::block_on(crate::mode::controller::set_stack_member(
                state,
                &AppType::Claude,
                &provider.id,
                true,
            ))
            .unwrap();
            let worker_state = state.clone();
            let (tx, rx) = std::sync::mpsc::channel();
            let worker = std::thread::spawn(move || {
                tx.send(ProviderService::delete(
                    &worker_state,
                    AppType::Claude,
                    "stack-delete",
                ))
                .unwrap();
            });
            rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("delete must not deadlock")
                .unwrap();
            worker.join().unwrap();
            assert!(!state
                .db
                .provider_exists("stack-delete", &AppType::Claude)
                .unwrap());
            assert!(!crate::mode::stack::is_member(&AppType::Claude, "stack-delete").unwrap());
        });
    }

    #[test]
    #[serial]
    fn universal_codex_sync_preserves_custom_toml_fields() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().unwrap();
            let mut parent = UniversalProvider::new(
                "toml-fields".into(),
                "Parent".into(),
                "newapi".into(),
                "https://new.test".into(),
                "key".into(),
            );
            parent.apps.codex = true;
            let mut child = parent.to_codex_provider().unwrap();
            child.settings_config["config"] = json!("# keep comment\nmodel = \"old\"\n[features]\ncustom = true\n[model_providers.custom]\nbase_url = \"https://old.test/v1\"\nrequest_max_retries = 7\n");
            state.db.save_provider(&AppType::Codex, &child).unwrap();
            state.db.save_universal_provider(&parent).unwrap();
            ProviderService::sync_universal_to_apps(state, &parent.id).unwrap();
            let saved = state
                .db
                .get_provider_by_id(&child.id, &AppType::Codex)
                .unwrap()
                .unwrap();
            let text = saved.settings_config["config"].as_str().unwrap();
            let config: toml::Value = text.parse().unwrap();
            assert!(text.contains("# keep comment"));
            assert_eq!(config["features"]["custom"].as_bool(), Some(true));
            assert_eq!(
                config["model_providers"]["custom"]["request_max_retries"].as_integer(),
                Some(7)
            );
            assert_eq!(
                config["model_providers"]["custom"]["base_url"].as_str(),
                Some("https://new.test/v1")
            );
        });
    }

    #[test]
    #[serial]
    fn codex_editor_metadata_does_not_change_other_app_previews() {
        with_test_home(|state, _home| {
            crate::settings::reload_settings().expect("reload settings");
            let settings = json!({"env": {
                "ANTHROPIC_BASE_URL": "https://relay.example",
                "ANTHROPIC_AUTH_TOKEN": "editor-key",
                "ANTHROPIC_MODEL": "claude-sonnet-5"
            }});
            let expected =
                ProviderService::editor_view(state, AppType::Claude, &settings, None).unwrap();
            let meta = ProviderMeta {
                provider_type: Some("codex_oauth".into()),
                ..Default::default()
            };
            let actual = ProviderService::editor_view_with_meta(
                state,
                AppType::Claude,
                &settings,
                None,
                Some("absent-provider"),
                Some(&meta),
            )
            .unwrap();
            assert_eq!(
                serde_json::to_value(actual).unwrap(),
                serde_json::to_value(expected).unwrap()
            );
        });
    }

    fn usage_script_with_credentials(
        api_key: Option<&str>,
        base_url: Option<&str>,
        template_type: Option<&str>,
    ) -> UsageScript {
        UsageScript {
            enabled: true,
            language: "javascript".to_string(),
            code: "return { remaining: 1, unit: 'USD' };".to_string(),
            timeout: Some(10),
            api_key: api_key.map(str::to_string),
            base_url: base_url.map(str::to_string),
            access_token: None,
            user_id: None,
            template_type: template_type.map(str::to_string),
            auto_query_interval: None,
            coding_plan_provider: None,
            access_key_id: Some("ak-test".to_string()),
            secret_access_key: Some("sk-test".to_string()),
            team_organization_id: None,
            team_project_id: None,
        }
    }

    fn codex_provider_with_usage(
        id: &str,
        base_url: &str,
        api_key: &str,
        usage_api_key: Option<&str>,
        usage_base_url: Option<&str>,
        template_type: Option<&str>,
    ) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            format!("Provider {id}"),
            codex_settings(base_url, api_key),
            None,
        );
        provider.meta = Some(ProviderMeta {
            usage_script: Some(usage_script_with_credentials(
                usage_api_key,
                usage_base_url,
                template_type,
            )),
            ..Default::default()
        });
        provider
    }

    fn managed_codex_provider(id: &str, account_id: &str) -> Provider {
        let mut provider = Provider::with_id(
            id.to_string(),
            format!("Managed {id}"),
            json!({
                "auth": {},
                "config": ""
            }),
            None,
        );
        provider.category = Some("official".to_string());
        provider.meta = Some(ProviderMeta {
            auth_binding: Some(AuthBinding {
                source: AuthBindingSource::ManagedAccount,
                auth_provider: Some("codex_oauth".to_string()),
                account_id: Some(account_id.to_string()),
            }),
            ..Default::default()
        });
        provider
    }

    fn opencode_provider(id: &str) -> Provider {
        Provider {
            id: id.to_string(),
            name: format!("Provider {id}"),
            settings_config: json!({
                "npm": "@ai-sdk/openai-compatible",
                "name": format!("Provider {id}"),
                "options": {
                    "baseURL": "https://api.example.com/v1",
                    "apiKey": "test-key"
                },
                "models": {
                    "gpt-4o": {
                        "name": "GPT-4o"
                    }
                }
            }),
            website_url: None,
            category: Some("custom".to_string()),
            created_at: Some(1),
            sort_index: Some(0),
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn opencode_omo_provider(id: &str, category: &str) -> Provider {
        let mut settings = serde_json::Map::new();
        settings.insert(
            "agents".to_string(),
            json!({
                "writer": {
                    "model": "gpt-4o-mini"
                }
            }),
        );
        if category == "omo" {
            settings.insert(
                "categories".to_string(),
                json!({
                    "default": ["writer"]
                }),
            );
        }
        settings.insert(
            "otherFields".to_string(),
            json!({
                "theme": "dark"
            }),
        );

        Provider {
            id: id.to_string(),
            name: format!("Provider {id}"),
            settings_config: Value::Object(settings),
            website_url: None,
            category: Some(category.to_string()),
            created_at: Some(1),
            sort_index: Some(0),
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    fn omo_config_path(home: &Path, category: &str) -> PathBuf {
        home.join(".config").join("opencode").join(match category {
            "omo" => crate::services::omo::STANDARD.preferred_filename,
            "omo-slim" => crate::services::omo::SLIM.preferred_filename,
            other => panic!("unexpected OMO category in test: {other}"),
        })
    }

    #[test]
    #[serial]
    fn add_clears_usage_credentials_that_match_provider_config() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-a",
                "https://api.a.example/v1/",
                "sk-a",
                Some(" sk-a "),
                Some(" https://api.a.example/v1/ "),
                None,
            );

            ProviderService::add(state, AppType::Codex, provider, false).expect("add provider");

            let saved = state
                .db
                .get_provider_by_id("codex-a", &AppType::Codex)
                .expect("query saved provider")
                .expect("saved provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key, None);
            assert_eq!(script.base_url, None);
        });
    }

    #[test]
    #[serial]
    fn update_preserves_usage_credentials_that_only_match_previous_config() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-usage-old",
                "https://api.a.example/v1/",
                "sk-a",
                Some("sk-a"),
                Some("https://api.a.example/v1/"),
                None,
            );
            state
                .db
                .save_provider(&AppType::Codex, &provider)
                .expect("seed provider with explicit usage credentials");

            let mut updated = provider.clone();
            updated.settings_config = codex_settings("https://api.b.example/v1/", "sk-b");

            ProviderService::update(state, AppType::Codex, None, updated)
                .expect("update provider main credentials");

            let saved = state
                .db
                .get_provider_by_id("codex-usage-old", &AppType::Codex)
                .expect("query updated provider")
                .expect("updated provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key.as_deref(), Some("sk-a"));
            assert_eq!(
                script.base_url.as_deref(),
                Some("https://api.a.example/v1/")
            );
            assert_eq!(
                saved.resolve_usage_credentials(&AppType::Codex),
                ("https://api.b.example/v1".to_string(), "sk-b".to_string())
            );
        });
    }

    #[test]
    #[serial]
    fn copied_provider_uses_edited_credentials_after_add_clears_mirrored_usage_credentials() {
        with_test_home(|state, _| {
            let copied_provider = codex_provider_with_usage(
                "codex-copy",
                "https://api.a.example/v1/",
                "sk-a",
                Some("sk-a"),
                Some("https://api.a.example/v1/"),
                None,
            );

            ProviderService::add(state, AppType::Codex, copied_provider, false)
                .expect("add copied provider");

            let saved_after_add = state
                .db
                .get_provider_by_id("codex-copy", &AppType::Codex)
                .expect("query copied provider")
                .expect("copied provider should exist");
            let script_after_add = saved_after_add
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");
            assert_eq!(script_after_add.api_key, None);
            assert_eq!(script_after_add.base_url, None);

            let mut edited_provider = saved_after_add.clone();
            edited_provider.settings_config = codex_settings("https://api.b.example/v1/", "sk-b");

            ProviderService::update(state, AppType::Codex, None, edited_provider)
                .expect("edit copied provider credentials");

            let saved_after_update = state
                .db
                .get_provider_by_id("codex-copy", &AppType::Codex)
                .expect("query edited provider")
                .expect("edited provider should exist");
            let script_after_update = saved_after_update
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script_after_update.api_key, None);
            assert_eq!(script_after_update.base_url, None);
            assert_eq!(
                saved_after_update.resolve_usage_credentials(&AppType::Codex),
                ("https://api.b.example/v1".to_string(), "sk-b".to_string())
            );
        });
    }

    #[test]
    #[serial]
    fn update_clears_usage_credentials_that_match_current_config() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-current",
                "https://api.a.example/v1",
                "sk-a",
                Some("sk-usage"),
                Some("https://usage.example/api"),
                None,
            );
            state
                .db
                .save_provider(&AppType::Codex, &provider)
                .expect("seed provider with distinct usage credentials");

            let mut updated = provider.clone();
            updated.settings_config = codex_settings("https://api.b.example/v1/", "sk-b");
            updated.meta = Some(ProviderMeta {
                usage_script: Some(usage_script_with_credentials(
                    Some(" sk-b "),
                    Some(" https://api.b.example/v1/ "),
                    None,
                )),
                ..Default::default()
            });

            ProviderService::update(state, AppType::Codex, None, updated)
                .expect("update provider with redundant usage credentials");

            let saved = state
                .db
                .get_provider_by_id("codex-current", &AppType::Codex)
                .expect("query updated provider")
                .expect("updated provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key, None);
            assert_eq!(script.base_url, None);
        });
    }

    #[test]
    #[serial]
    fn add_preserves_distinct_usage_credentials() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-distinct",
                "https://api.main.example/v1",
                "sk-main",
                Some("sk-usage"),
                Some("https://usage.example/api"),
                None,
            );

            ProviderService::add(state, AppType::Codex, provider, false).expect("add provider");

            let saved = state
                .db
                .get_provider_by_id("codex-distinct", &AppType::Codex)
                .expect("query saved provider")
                .expect("saved provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key.as_deref(), Some("sk-usage"));
            assert_eq!(
                script.base_url.as_deref(),
                Some("https://usage.example/api")
            );
        });
    }

    #[test]
    #[serial]
    fn add_does_not_clear_token_plan_credentials() {
        with_test_home(|state, _| {
            let provider = codex_provider_with_usage(
                "codex-token-plan",
                "https://api.plan.example/v1",
                "sk-plan",
                Some("sk-plan"),
                Some("https://api.plan.example/v1"),
                Some("token_plan"),
            );

            ProviderService::add(state, AppType::Codex, provider, false).expect("add provider");

            let saved = state
                .db
                .get_provider_by_id("codex-token-plan", &AppType::Codex)
                .expect("query saved provider")
                .expect("saved provider should exist");
            let script = saved
                .meta
                .as_ref()
                .and_then(|meta| meta.usage_script.as_ref())
                .expect("usage script should remain");

            assert_eq!(script.api_key.as_deref(), Some("sk-plan"));
            assert_eq!(
                script.base_url.as_deref(),
                Some("https://api.plan.example/v1")
            );
            assert_eq!(script.access_key_id.as_deref(), Some("ak-test"));
            assert_eq!(script.secret_access_key.as_deref(), Some("sk-test"));
        });
    }

    #[test]
    fn retired_provider_is_rejected_before_client_specific_writes() {
        let provider: Provider = serde_json::from_value(json!({
            "id": "retired", "name": "Retired", "settingsConfig": {},
            "meta": {"providerType": "xai_oauth"}
        }))
        .unwrap();
        for app in AppType::all() {
            let error = ProviderService::validate_provider_settings(&app, &provider).unwrap_err();
            assert!(
                error.to_string().contains("no longer supported"),
                "{app}: {error}"
            );
        }
    }

    #[test]
    fn validate_provider_settings_rejects_missing_auth() {
        let provider = Provider::with_id(
            "codex".into(),
            "Codex".into(),
            json!({ "config": "base_url = \"https://example.com\"" }),
            None,
        );
        let err = ProviderService::validate_provider_settings(&AppType::Codex, &provider)
            .expect_err("missing auth should be rejected");
        assert!(
            err.to_string().contains("auth"),
            "expected auth error, got {err:?}"
        );
    }

    #[test]
    #[serial]
    fn add_accepts_multiple_unbound_codex_official_cards() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            state
                .db
                .init_default_official_providers()
                .expect("seed official providers");
            let fixed_id = crate::database::CODEX_OFFICIAL_PROVIDER_ID;
            state
                .db
                .set_current_provider(&AppType::Codex, fixed_id)
                .expect("set database current");
            crate::settings::set_current_provider(&AppType::Codex, Some(fixed_id))
                .expect("set local current");

            for id in ["follow-login-a", "follow-login-b"] {
                let mut provider = Provider::with_id(
                    id.to_string(),
                    id.to_string(),
                    json!({ "auth": {}, "config": "" }),
                    None,
                );
                provider.category = Some("official".to_string());
                ProviderService::add(state, AppType::Codex, provider, false)
                    .expect("add unbound Official card");
            }

            let providers = state
                .db
                .get_all_providers(&AppType::Codex)
                .expect("read providers");
            assert!(providers.contains_key("follow-login-a"));
            assert!(providers.contains_key("follow-login-b"));
        });
    }

    #[test]
    #[serial]
    fn update_keeps_official_provider_id_when_binding_and_unbinding() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-access-token",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed account");
            });

            let provider_id = crate::database::CODEX_OFFICIAL_PROVIDER_ID;
            let mut unbound = Provider::with_id(
                provider_id.to_string(),
                "OpenAI Official".to_string(),
                json!({ "auth": {}, "config": "" }),
                None,
            );
            unbound.category = Some("official".to_string());
            state
                .db
                .save_provider(&AppType::Codex, &unbound)
                .expect("save unbound card");
            state
                .db
                .set_current_provider(&AppType::Codex, provider_id)
                .expect("set database current");
            crate::settings::set_current_provider(&AppType::Codex, Some(provider_id))
                .expect("set local current");

            let mut bound = managed_codex_provider(provider_id, "acct-managed");
            bound.name = unbound.name.clone();
            ProviderService::update(state, AppType::Codex, Some(provider_id), bound)
                .expect("bind managed account");

            let saved_bound = state
                .db
                .get_provider_by_id(provider_id, &AppType::Codex)
                .expect("query bound card")
                .expect("bound card should keep its ID");
            assert_eq!(
                ProviderService::managed_codex_oauth_account_id(&saved_bound).as_deref(),
                Some("acct-managed")
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Codex)
                    .expect("read database current")
                    .as_deref(),
                Some(provider_id)
            );
            assert_eq!(
                crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                Some(provider_id)
            );

            // 旧版「统一会话历史」注入进 live、又被回填进行里的形态，保存时剥掉。
            unbound.settings_config["config"] = Value::String("model_provider = \"custom\"\n\n[model_providers.custom]\nname = \"OpenAI\"\nrequires_openai_auth = true\nsupports_websockets = true\nwire_api = \"responses\"\n".to_string());
            ProviderService::update(state, AppType::Codex, Some(provider_id), unbound)
                .expect("unbind managed account");

            let saved_unbound = state
                .db
                .get_provider_by_id(provider_id, &AppType::Codex)
                .expect("query unbound card")
                .expect("unbound card should keep its ID");
            assert!(ProviderService::managed_codex_oauth_account_id(&saved_unbound).is_none());
            assert_eq!(saved_unbound.settings_config["config"], json!(""));
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Codex)
                    .expect("read database current")
                    .as_deref(),
                Some(provider_id)
            );
            assert_eq!(
                crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                Some(provider_id)
            );
        });
    }

    /// 造一个「已被污染」的现场：片段里带 A 账号的凭据 + 一个合法可共享键。
    #[test]
    fn sensitive_key_matcher_covers_common_credential_namings() {
        for key in [
            // 裸 `_KEY`：最常见的写法，却曾被"只枚举 `_API_KEY` 这些子类"漏在外面
            "OPENAI_KEY",
            "GROQ_KEY",
            "XAI_KEY",
            // 不带分隔符的复合写法
            "VOLC_ACCESSKEY",
            "ALIYUN_SECRETKEY",
            "SOME_APITOKEN",
            // personal access token：既不含 TOKEN 也不含 KEY
            "GITHUB_PAT",
            "gitlab_pat",
            // 口令类缩写
            "MYSQL_PWD",
            "DB_PASS",
            "GPG_PASSPHRASE",
            "AWS_CREDS",
            // 发往上游的自定义请求头、Cookie、Authorization
            "ANTHROPIC_CUSTOM_HEADERS",
            "GEMINI_CLI_CUSTOM_HEADERS",
            "headers",
            "UPSTREAM_COOKIE",
            "PROXY_AUTHORIZATION",
        ] {
            assert!(
                ProviderService::is_sensitive_config_key(key),
                "{key} must be treated as a credential"
            );
        }

        // 后缀必须带下划线，不能把正常配置一起卷进来
        for key in [
            "PATH",
            "OLDPWD",
            "GEMINI_COMPAT",
            "SSL_BYPASS",
            "GEMINI_TIMEOUT_MS",
            "CLAUDE_CODE_MAX_OUTPUT_TOKENS",
            // 名字带 HEADER 但不是发往上游的凭据：普通开关、用户自己的遥测端点
            "CLAUDE_CODE_ATTRIBUTION_HEADER",
            "OTEL_EXPORTER_OTLP_HEADERS",
        ] {
            assert!(
                !ProviderService::is_sensitive_config_key(key),
                "{key} is ordinary shareable config and must not be stripped"
            );
        }
    }

    /// Saving the active provider while takeover has never been enabled must
    /// rewrite the real live file immediately.
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_writes_live_when_proxy_never_enabled() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.old.example"
                }
            }),
            None,
        );
        db.save_provider(&AppType::Claude, &original)
            .expect("save provider");
        db.set_current_provider(&AppType::Claude, "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_live_for_state(&state, &AppType::Claude, &original).expect("seed live file");

        let mut updated = original.clone();
        updated.settings_config["env"]["ANTHROPIC_BASE_URL"] =
            Value::String("https://api.new.example".into());
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update current provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"].as_str(),
            Some("https://api.new.example")
        );
    }

    /// 编辑当前供应商、去掉它的独有字段：live 里 CC Switch 写进去的那个值随之删掉，
    /// 用户自己的键不动。
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_drops_the_compat_switch_it_no_longer_has() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "ds".into(),
            "DeepSeek".into(),
            json!({ "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.example/anthropic",
                "CLAUDE_CODE_DISABLE_ARTIFACT": "1"
            }}),
            None,
        );
        db.save_provider(&AppType::Claude, &original)
            .expect("save provider");
        ProviderService::switch(&state, AppType::Claude, "ds").expect("switch");
        let mut live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        live["env"]["DEBUG"] = json!("1");
        write_json_file(&get_claude_settings_path(), &live).expect("user edit");

        let mut updated = original.clone();
        updated.settings_config["env"]
            .as_object_mut()
            .expect("env")
            .remove("CLAUDE_CODE_DISABLE_ARTIFACT");
        ProviderService::update(&state, AppType::Claude, None, updated).expect("update");

        // 开着的会话先读到空串（撤掉旧值），几秒后再删
        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(live["env"]["CLAUDE_CODE_DISABLE_ARTIFACT"], json!(""));
        claude_direct::settle_tombstones();
        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live,
            json!({ "env": {
                "ANTHROPIC_BASE_URL": "https://api.deepseek.example/anthropic",
                "DEBUG": "1"
            }})
        );
    }

    /// 文件已经写成目标供应商、指针还没改时崩溃：下次启动按 pending 补完指针，
    /// 文件和指针重新一致。
    #[tokio::test]
    #[serial]
    async fn claude_switch_interrupted_before_the_pointer_rolls_forward() {
        use crate::mode::operation::failpoint;

        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        for (id, url) in [("a", "https://a.example"), ("b", "https://b.example")] {
            let provider = Provider::with_id(
                id.into(),
                id.into(),
                json!({ "env": { "ANTHROPIC_BASE_URL": url } }),
                None,
            );
            db.save_provider(&AppType::Claude, &provider)
                .expect("save provider");
        }
        ProviderService::switch(&state, AppType::Claude, "a").expect("switch to a");

        failpoint::crash_at(Some("published:0"));
        let result = ProviderService::switch(&state, AppType::Claude, "b");
        failpoint::crash_at(None);
        assert!(result.is_err(), "the injected crash surfaces");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"],
            json!("https://b.example")
        );
        assert_eq!(
            db.get_current_provider(&AppType::Claude)
                .expect("current")
                .as_deref(),
            Some("a"),
            "the pointer has not moved yet"
        );

        crate::mode::operation::recover_on_startup(&db);
        assert_eq!(
            db.get_current_provider(&AppType::Claude)
                .expect("current")
                .as_deref(),
            Some("b")
        );
        assert_eq!(
            crate::settings::get_current_provider(&AppType::Claude).as_deref(),
            Some("b")
        );
    }

    /// A stale backup row must be refreshed but must not divert the live write.
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_writes_live_when_backup_row_is_stale() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.old.example"
                }
            }),
            None,
        );
        db.save_provider(&AppType::Claude, &original)
            .expect("save provider");
        db.set_current_provider(&AppType::Claude, "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_live_for_state(&state, &AppType::Claude, &original).expect("seed live file");
        db.save_live_backup(
            &AppType::Claude,
            &serde_json::to_string(&original.settings_config).expect("serialize backup"),
        )
        .await
        .expect("seed stale backup");
        assert!(!state.proxy_service.is_running().await);

        let mut updated = original.clone();
        updated.settings_config["env"]["ANTHROPIC_BASE_URL"] =
            Value::String("https://api.new.example".into());
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update current provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"].as_str(),
            Some("https://api.new.example"),
            "a leftover backup row is not proxy mode: the edit goes to live"
        );
    }

    /// An enabled flag left behind by an interrupted teardown is not enough to
    /// suppress a live write when neither placeholder nor backup evidence exists.
    #[tokio::test]
    #[serial]
    async fn update_current_claude_provider_ignores_enabled_flag_without_evidence() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());
        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_AUTH_TOKEN": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.old.example"
                }
            }),
            None,
        );
        db.save_provider(&AppType::Claude, &original)
            .expect("save provider");
        db.set_current_provider(&AppType::Claude, "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_live_for_state(&state, &AppType::Claude, &original).expect("seed live file");
        let mut config = db
            .get_proxy_config_for_app(&AppType::Claude)
            .await
            .expect("read proxy config");
        config.enabled = true;
        db.update_proxy_config_for_app(config)
            .await
            .expect("leave enabled flag set");
        assert!(!state.proxy_service.is_running().await);

        let mut updated = original.clone();
        updated.settings_config["env"]["ANTHROPIC_BASE_URL"] =
            Value::String("https://api.new.example".into());
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update current provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        assert_eq!(
            live["env"]["ANTHROPIC_BASE_URL"].as_str(),
            Some("https://api.new.example")
        );
    }

    #[test]
    fn extract_claude_common_config_strips_all_credentials_keeps_shareable() {
        // env 混入多种凭据（Anthropic/OpenRouter/Google/OpenAI/Gemini + AWS/Vertex）
        // 与可共享配置；顶层混入非标准的 apiKey/api_key 凭据与正常设置。
        let settings = json!({
            "env": {
                "ANTHROPIC_API_KEY": "sk-ant",
                "ANTHROPIC_AUTH_TOKEN": "tok-ant",
                "OPENROUTER_API_KEY": "sk-or",
                "GOOGLE_API_KEY": "g-key",
                "OPENAI_API_KEY": "sk-oai",
                "GEMINI_API_KEY": "g-gem",
                "AWS_ACCESS_KEY_ID": "AKIA",
                "AWS_SECRET_ACCESS_KEY": "secret",
                "AWS_SESSION_TOKEN": "sess",
                "GOOGLE_APPLICATION_CREDENTIALS": "/path/creds.json",
                "AWS_BEARER_TOKEN_BEDROCK": "bedrock-tok",
                "ANTHROPIC_BASE_URL": "https://example.com",
                "ANTHROPIC_MODEL": "claude-x",
                "CLAUDE_CODE_SUBAGENT_MODEL": "gpt-5.4-mini",
                "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "400000",
                "CLAUDE_CODE_AUTO_COMPACT_WINDOW": "400000",
                // 可共享、非机密配置（复数 _TOKENS 不应被误剥）
                "ENABLE_TOOL_SEARCH": "true",
                "CLAUDE_CODE_MAX_OUTPUT_TOKENS": "8192"
            },
            "apiKey": "sk-top",
            "api_key": "sk-top2",
            "theme": "dark",
            "includeCoAuthoredBy": false
        });

        let snippet = ProviderService::extract_claude_common_config(&settings)
            .expect("extract should succeed");
        let value: Value = serde_json::from_str(&snippet).expect("snippet is valid JSON");

        // 所有凭据都不得出现在共享片段里
        let env = value.get("env");
        for leaked in [
            "ANTHROPIC_API_KEY",
            "ANTHROPIC_AUTH_TOKEN",
            "OPENROUTER_API_KEY",
            "GOOGLE_API_KEY",
            "OPENAI_API_KEY",
            "GEMINI_API_KEY",
            "AWS_ACCESS_KEY_ID",
            "AWS_SECRET_ACCESS_KEY",
            "AWS_SESSION_TOKEN",
            "GOOGLE_APPLICATION_CREDENTIALS",
            "AWS_BEARER_TOKEN_BEDROCK",
        ] {
            assert!(
                env.and_then(|e| e.get(leaked)).is_none(),
                "credential {leaked} must not leak into common config"
            );
        }
        assert!(
            value.get("apiKey").is_none() && value.get("api_key").is_none(),
            "top-level credentials must be stripped"
        );

        // 端点/模型（provider-specific 非机密）也应剥掉
        assert!(env.and_then(|e| e.get("ANTHROPIC_BASE_URL")).is_none());
        assert!(env.and_then(|e| e.get("ANTHROPIC_MODEL")).is_none());
        assert!(env
            .and_then(|e| e.get("CLAUDE_CODE_SUBAGENT_MODEL"))
            .is_none());
        assert!(env
            .and_then(|e| e.get("CLAUDE_CODE_MAX_CONTEXT_TOKENS"))
            .is_none());
        assert!(env
            .and_then(|e| e.get("CLAUDE_CODE_AUTO_COMPACT_WINDOW"))
            .is_none());

        // 可共享的非机密配置必须保留（含复数 _TOKENS 不被误剥）
        assert_eq!(
            env.and_then(|e| e.get("ENABLE_TOOL_SEARCH"))
                .and_then(|v| v.as_str()),
            Some("true")
        );
        assert_eq!(
            env.and_then(|e| e.get("CLAUDE_CODE_MAX_OUTPUT_TOKENS"))
                .and_then(|v| v.as_str()),
            Some("8192")
        );
        assert_eq!(value.get("theme").and_then(|v| v.as_str()), Some("dark"));
        assert_eq!(value.get("includeCoAuthoredBy"), Some(&json!(false)));
    }

    /// 关键字段（协议选择器、Bedrock/Vertex 区域、`/model` 的选择等）不进共享片段；
    /// 同在 `CLAUDE_CODE_USE_` 前缀下、与供应商无关的开关照常共享。
    #[test]
    fn extract_claude_common_config_keeps_key_fields_per_provider() {
        let settings = json!({
            "env": {
                "CLAUDE_CODE_USE_BEDROCK": "1",
                "CLAUDE_CODE_USE_VERTEX": "1",
                "CLAUDE_CODE_SKIP_BEDROCK_AUTH": "1",
                "AWS_REGION": "us-west-2",
                "AWS_PROFILE": "work",
                "CLOUD_ML_REGION": "us-east5",
                "VERTEX_REGION_CLAUDE_4_0_OPUS": "europe-west1",
                "ANTHROPIC_VERTEX_PROJECT_ID": "my-project",
                "ANTHROPIC_SMALL_FAST_MODEL": "haiku",
                "CLAUDE_CODE_SUBAGENT_MODEL_FORCE": "1",
                "CLAUDE_CODE_USE_POWERSHELL_TOOL": "1",
                "CLAUDE_CODE_ATTRIBUTION_HEADER": "0",
                "DISABLE_TELEMETRY": "1"
            },
            "model": "opus",
            "fallbackModel": "sonnet",
            "apiKeyHelper": "~/bin/key.sh",
            "awsAuthRefresh": "aws sso login",
            "hooks": { "Stop": [] },
            "theme": "dark"
        });

        let snippet = ProviderService::extract_claude_common_config(&settings)
            .expect("extract should succeed");
        let value: Value = serde_json::from_str(&snippet).expect("snippet is valid JSON");

        assert_eq!(
            value,
            json!({
                "env": {
                    "CLAUDE_CODE_SUBAGENT_MODEL_FORCE": "1",
                    "CLAUDE_CODE_USE_POWERSHELL_TOOL": "1",
                    "CLAUDE_CODE_ATTRIBUTION_HEADER": "0",
                    "DISABLE_TELEMETRY": "1"
                },
                "hooks": { "Stop": [] },
                "theme": "dark"
            })
        );
        assert!(
            snippet.find("CLAUDE_CODE_USE_POWERSHELL_TOOL") < snippet.find("DISABLE_TELEMETRY"),
            "removing keys must not reorder the ones that stay: {snippet}"
        );
    }

    /// Regression for issue #4272: Fable tier env keys must not enter the shared
    /// Claude common-config snippet (same class as haiku/sonnet/opus model pins).
    #[test]
    fn extract_claude_common_config_strips_fable_model_env_keys() {
        let settings = json!({
            "env": {
                "ANTHROPIC_DEFAULT_HAIKU_MODEL": "haiku-mapped",
                "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME": "Haiku Mapped",
                "ANTHROPIC_DEFAULT_SONNET_MODEL": "sonnet-mapped[1M]",
                "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME": "Sonnet Mapped",
                "ANTHROPIC_DEFAULT_OPUS_MODEL": "opus-mapped[1M]",
                "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME": "Opus Mapped",
                "ANTHROPIC_DEFAULT_FABLE_MODEL": "deepseek-v4-flash[1M]",
                "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME": "deepseek-v4-flash",
                "ANTHROPIC_MODEL": "default-mapped",
                "ENABLE_TOOL_SEARCH": "true"
            },
            "theme": "dark"
        });

        let snippet = ProviderService::extract_claude_common_config(&settings)
            .expect("extract should succeed");
        let value: Value = serde_json::from_str(&snippet).expect("snippet is valid JSON");
        let env = value.get("env");

        for stripped in [
            "ANTHROPIC_DEFAULT_HAIKU_MODEL",
            "ANTHROPIC_DEFAULT_HAIKU_MODEL_NAME",
            "ANTHROPIC_DEFAULT_SONNET_MODEL",
            "ANTHROPIC_DEFAULT_SONNET_MODEL_NAME",
            "ANTHROPIC_DEFAULT_OPUS_MODEL",
            "ANTHROPIC_DEFAULT_OPUS_MODEL_NAME",
            "ANTHROPIC_DEFAULT_FABLE_MODEL",
            "ANTHROPIC_DEFAULT_FABLE_MODEL_NAME",
            "ANTHROPIC_MODEL",
        ] {
            assert!(
                env.and_then(|e| e.get(stripped)).is_none(),
                "provider-specific model key {stripped} must not enter common config"
            );
        }

        assert_eq!(
            env.and_then(|e| e.get("ENABLE_TOOL_SEARCH"))
                .and_then(|v| v.as_str()),
            Some("true")
        );
        assert_eq!(value.get("theme").and_then(|v| v.as_str()), Some("dark"));
    }

    #[test]
    fn extract_codex_common_config_strips_provider_fields_and_injected_artifacts() {
        // 顶层 experimental_bearer_token 模拟无活跃路由时的 fallback 注入；
        // web_search = "disabled" 是 cc-switch 对黑名单网关注入的哨兵；
        // 顶层 wire_api 模拟无 model_provider 时的 fallback 写法；
        // [mcp.servers] 是历史错误格式，sync_all_enabled 清不掉它。
        let config_toml = r#"model_provider = "azure"
model = "gpt-4"
wire_api = "chat"
disable_response_storage = true
model_reasoning_effort = "high"
approval_policy = "on-request"
experimental_bearer_token = "sk-live-secret"
model_catalog_json = "cc-switch-model-catalog.json"
web_search = "disabled"

[agents]
default_subagent_model = "gpt-4-mini"
max_threads = 4

[model_providers.azure]
name = "Azure OpenAI"
base_url = "https://azure.example/v1"
wire_api = "responses"

[mcp_servers.my_server]
base_url = "http://localhost:8080"

[mcp.servers.legacy_server]
command = "legacy-cmd"
"#;

        let settings = json!({ "config": config_toml });
        let extracted = ProviderService::extract_codex_common_config(&settings)
            .expect("extract_codex_common_config should succeed");

        assert!(
            !extracted
                .lines()
                .any(|line| line.trim_start().starts_with("model_provider")),
            "should remove top-level model_provider"
        );
        assert!(
            !extracted
                .lines()
                .any(|line| line.trim_start().starts_with("model =")),
            "should remove top-level model"
        );
        assert!(
            !extracted.contains("[model_providers"),
            "should remove entire model_providers table"
        );
        // MCP 归 DB mcp_servers 表所有，不得进共享片段（含历史错误格式 [mcp.servers]）
        assert!(
            !extracted.contains("mcp_servers") && !extracted.contains("http://localhost:8080"),
            "should strip mcp_servers from the shared snippet, got: {extracted}"
        );
        assert!(
            !extracted.contains("[mcp") && !extracted.contains("legacy-cmd"),
            "should strip the legacy [mcp.servers] form from the shared snippet, got: {extracted}"
        );
        // 顶层 wire_api 是供应商路由语义（model_providers 整表已剥，
        // 剩余任何 wire_api 都意味着泄漏）
        assert!(
            !extracted.contains("wire_api"),
            "should strip top-level wire_api from the shared snippet, got: {extracted}"
        );
        // 注入产物不得进共享片段（bearer token 泄漏为密钥级问题）
        assert!(
            !extracted.contains("experimental_bearer_token")
                && !extracted.contains("sk-live-secret"),
            "should strip top-level fallback bearer token, got: {extracted}"
        );
        assert!(
            !extracted.contains("model_catalog_json"),
            "should strip catalog projection pointer, got: {extracted}"
        );
        assert!(
            !extracted.contains("web_search"),
            "should strip the cc-switch web_search disabled sentinel, got: {extracted}"
        );
        // 关键字段归供应商（片段已冻结，收进去会从行里剥掉、再也写不回 live）
        for key in [
            "disable_response_storage",
            "model_reasoning_effort",
            "default_subagent_model",
        ] {
            assert!(
                !extracted.contains(key),
                "key field {key} must stay with the provider, got: {extracted}"
            );
        }
        // 真正可共享的键保留
        assert!(
            extracted.contains("approval_policy = \"on-request\"")
                && extracted.contains("max_threads = 4"),
            "shareable keys must survive extraction, got: {extracted}"
        );
    }

    #[test]
    fn extract_codex_common_config_keeps_user_set_web_search() {
        let config_toml = "web_search = \"enabled\"\ndisable_response_storage = true\n";
        let settings = json!({ "config": config_toml });
        let extracted = ProviderService::extract_codex_common_config(&settings)
            .expect("extract should succeed");
        assert!(
            extracted.contains("web_search = \"enabled\""),
            "a user-set web_search value is a shareable preference, got: {extracted}"
        );
    }

    #[tokio::test]
    #[serial]
    async fn editing_the_routed_claude_provider_rewrites_the_proxy_contract() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());

        let original = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "token-a",
                    "ANTHROPIC_BASE_URL": "https://api.a.example",
                    "ANTHROPIC_MODEL": "model-a"
                }
            }),
            None,
        );
        db.save_provider(&AppType::Claude, &original)
            .expect("save provider");
        db.set_current_provider(&AppType::Claude, "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Claude, Some("p1"))
            .expect("set local current provider");
        write_json_file(
            &get_claude_settings_path(),
            &json!({
                "env": {
                    "ANTHROPIC_BASE_URL": "https://api.a.example",
                    "ANTHROPIC_API_KEY": "token-a",
                    "ANTHROPIC_MODEL": "model-a"
                },
                "permissions": { "allow": ["Bash"] }
            }),
        )
        .expect("seed live file");

        db.update_proxy_config(ProxyConfig {
            listen_port: 0,
            ..Default::default()
        })
        .await
        .expect("update proxy config");
        crate::mode::controller::enter(&state, &AppType::Claude, false)
            .await
            .expect("enter routing mode");
        let proxy_url = state
            .proxy_service
            .build_proxy_urls()
            .await
            .expect("proxy url")
            .0;

        let updated = Provider::with_id(
            "p1".into(),
            "Claude A".into(),
            json!({
                "env": {
                    "ANTHROPIC_API_KEY": "token-updated",
                    "ANTHROPIC_BASE_URL": "https://api.updated.example",
                    "ANTHROPIC_MODEL": "model-updated"
                }
            }),
            None,
        );
        ProviderService::update(&state, AppType::Claude, None, updated)
            .expect("update routed provider");

        let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
        let env = &live["env"];
        // 行里只配了 `ANTHROPIC_API_KEY`（供上游跳用 x-api-key），本地占位符仍落在
        // `ANTHROPIC_AUTH_TOKEN`，且本地不出现 `ANTHROPIC_API_KEY`（#8104）。
        assert_eq!(env["ANTHROPIC_AUTH_TOKEN"], "PROXY_MANAGED");
        assert_eq!(
            env.get("ANTHROPIC_API_KEY"),
            None,
            "route mode must not present the placeholder as a custom API key"
        );
        assert_eq!(env["ANTHROPIC_BASE_URL"], proxy_url.as_str());
        assert!(env.get("ANTHROPIC_MODEL").is_none(), "{live}");
        assert_eq!(
            env["ANTHROPIC_DEFAULT_SONNET_MODEL_NAME"], "model-updated",
            "the contract's display names follow the edited provider"
        );
        assert_eq!(
            live["permissions"],
            json!({ "allow": ["Bash"] }),
            "user settings in live are left alone"
        );

        state
            .proxy_service
            .stop()
            .await
            .expect("stop proxy service");
    }

    #[tokio::test]
    #[serial]
    async fn update_current_codex_provider_refreshes_and_clears_catalog_during_takeover() {
        let _home = TempHome::new();
        crate::settings::reload_settings().expect("reload settings");

        let db = Arc::new(Database::memory().expect("init db"));
        let state = AppState::new(db.clone());

        let mut original = Provider::with_id(
            "p1".into(),
            "Codex A".into(),
            json!({
                "auth": { "OPENAI_API_KEY": "token-a" },
                "config": r#"model_provider = "custom"
model = "old-model"

[model_providers.custom]
name = "Codex A"
base_url = "https://api.a.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#,
                "modelCatalog": {
                    "models": [{ "model": "old-model" }]
                }
            }),
            None,
        );
        original.meta = Some(ProviderMeta {
            api_format: Some("openai_responses".into()),
            ..Default::default()
        });
        db.save_provider(&AppType::Codex, &original)
            .expect("save provider");
        db.set_current_provider(&AppType::Codex, "p1")
            .expect("set current provider");
        crate::settings::set_current_provider(&AppType::Codex, Some("p1"))
            .expect("set local current provider");

        db.update_proxy_config(ProxyConfig {
            listen_port: 0,
            ..Default::default()
        })
        .await
        .expect("update proxy config");
        crate::mode::controller::enter(&state, &AppType::Codex, false)
            .await
            .expect("enter routing mode");
        assert!(
            state
                .proxy_service
                .live_has_proxy_placeholder(&AppType::Codex),
            "Codex live config should carry the proxy contract"
        );

        let mut updated = original.clone();
        updated.settings_config["config"] = json!(
            r#"model_provider = "custom"
model = "gpt-5.4"

[model_providers.custom]
name = "Codex A"
base_url = "https://api.updated.example/v1"
wire_api = "responses"
requires_openai_auth = true
"#
        );
        updated.settings_config["modelCatalog"] = json!({
            "models": [{ "model": "gpt-5.4", "displayName": "GPT 5.4" }]
        });

        ProviderService::update(&state, AppType::Codex, None, updated.clone())
            .expect("update current Codex provider mapping");

        let catalog_path = crate::codex_config::get_codex_model_catalog_path();
        let catalog: Value = read_json_file(&catalog_path).expect("read generated catalog");
        assert_eq!(catalog["models"][0]["slug"], "gpt-5.4");
        assert_eq!(
            catalog["models"][0]["input_modalities"],
            json!(["text", "image"]),
            "unknown/GPT models must fail open to image input"
        );
        let live_config = fs::read_to_string(crate::codex_config::get_codex_config_path())
            .expect("read Codex config.toml");
        assert!(live_config.contains("model_catalog_json"));

        updated.settings_config["modelCatalog"] = json!({ "models": [] });
        ProviderService::update(&state, AppType::Codex, None, updated)
            .expect("remove current Codex provider mapping");

        let live_config = fs::read_to_string(crate::codex_config::get_codex_config_path())
            .expect("read Codex config.toml after mapping removal");
        assert!(
            !live_config.contains("model_catalog_json"),
            "removing mappings during takeover must clear the stale catalog pointer"
        );

        state
            .proxy_service
            .stop()
            .await
            .expect("stop proxy service");
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_skips_db_only_opencode_provider() {
        with_test_home(|state, _| {
            let provider = opencode_provider("db-only-opencode");
            ProviderService::add(state, AppType::OpenCode, provider.clone(), false)
                .expect("seed db-only opencode provider");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                .expect("sync additive opencode providers");

            let live_providers = crate::opencode_config::get_providers()
                .expect("read opencode providers after sync");
            assert!(
                !live_providers.contains_key(&provider.id),
                "db-only opencode provider should not be written to live during sync"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_preserves_legacy_live_opencode_provider() {
        with_test_home(|state, _| {
            let provider = opencode_provider("legacy-opencode");
            crate::opencode_config::set_provider(&provider.id, provider.settings_config.clone())
                .expect("seed opencode live provider");
            state
                .db
                .save_provider(&AppType::OpenCode, &provider)
                .expect("seed legacy opencode provider in db");

            let mut updated = provider.clone();
            updated.settings_config["options"]["apiKey"] = Value::String("updated-key".to_string());
            state
                .db
                .save_provider(&AppType::OpenCode, &updated)
                .expect("update legacy opencode provider in db");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                .expect("sync legacy opencode provider");

            let live_providers =
                crate::opencode_config::get_providers().expect("read opencode providers");
            assert_eq!(
                live_providers
                    .get(&provider.id)
                    .and_then(|config| config.get("options"))
                    .and_then(|options| options.get("apiKey")),
                Some(&Value::String("updated-key".to_string())),
                "legacy provider that already exists in live should still be synced"
            );
        });
    }

    #[test]
    #[serial]
    fn sync_current_provider_for_app_restores_legacy_opencode_provider_after_live_reset() {
        with_test_home(|state, _| {
            let provider = opencode_provider("legacy-opencode-reset");
            state
                .db
                .save_provider(&AppType::OpenCode, &provider)
                .expect("seed legacy opencode provider in db");

            ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                .expect("sync legacy opencode provider after reset");

            let live_providers =
                crate::opencode_config::get_providers().expect("read opencode providers");
            assert!(
                live_providers.contains_key(&provider.id),
                "legacy opencode provider should be restored when live config is reset"
            );
        });
    }

    #[test]
    #[serial]
    fn add_first_managed_codex_with_reauth_required_account_is_rejected() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_access_token("acct-legacy", "managed-token", None)
                    .await
                    .expect("seed legacy account without id_token");
            });
            let provider = managed_codex_provider("managed-legacy", "acct-legacy");

            let error = ProviderService::add(state, AppType::Codex, provider.clone(), false)
                .expect_err("reauth-required account must not be written to live auth");
            assert!(
                error.to_string().contains("id_token"),
                "backend should require re-login even if the frontend gate is bypassed: {error}"
            );
            assert!(state
                .db
                .get_provider_by_id(&provider.id, &AppType::Codex)
                .expect("query provider")
                .is_none());
            assert!(!crate::codex_config::get_codex_auth_path().exists());
        });
    }

    #[test]
    #[serial]
    fn switch_from_managed_codex_official_to_unbound_clears_live_without_backfilling_token() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-token",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed Codex OAuth account");
            });

            let mut managed = Provider::with_id(
                "managed-official".to_string(),
                "Managed Official".to_string(),
                json!({
                    "auth": {},
                    "config": ""
                }),
                None,
            );
            managed.category = Some("official".to_string());
            managed.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-managed".to_string()),
                }),
                ..Default::default()
            });

            let mut unbound = Provider::with_id(
                "unbound-official".to_string(),
                "Unbound Official".to_string(),
                json!({
                    "auth": {},
                    "config": ""
                }),
                None,
            );
            unbound.category = Some("official".to_string());

            state
                .db
                .save_provider(&AppType::Codex, &managed)
                .expect("save managed provider");
            state
                .db
                .save_provider(&AppType::Codex, &unbound)
                .expect("save unbound provider");

            ProviderService::switch(state, AppType::Codex, "managed-official")
                .expect("switch to managed official");
            let live_auth: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read managed live auth");
            assert_eq!(
                live_auth
                    .pointer("/tokens/access_token")
                    .and_then(Value::as_str),
                Some("managed-token"),
                "managed switch should write the selected ChatGPT token to live auth"
            );

            // Simulate a bare Codex CLI self-refresh. The app marker still
            // describes the pre-refresh write, while both access and refresh
            // token material on disk have rotated.
            let rotated_id_token = crate::codex_config::test_codex_id_token("managed-user");
            let rotated_live_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "acct-managed",
                "cli-rotated-access",
                Some(&rotated_id_token),
                "cli-rotated-refresh",
                "2099-01-02T03:04:05Z",
            );
            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &rotated_live_auth,
            )
            .expect("simulate Codex CLI token rotation");

            ProviderService::switch(state, AppType::Codex, "unbound-official")
                .expect("switch to unbound official");

            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "switching to an unbound official provider should clear the recorded managed live auth"
            );
            assert_eq!(
                crate::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-managed")
                )
                .as_deref(),
                Some("cli-rotated-refresh"),
                "switch-away must adopt the CLI-rotated refresh token before deleting live auth"
            );

            let saved_managed = state
                .db
                .get_provider_by_id("managed-official", &AppType::Codex)
                .expect("query managed provider")
                .expect("managed provider should exist");
            assert_eq!(
                saved_managed.settings_config.get("auth"),
                Some(&json!({})),
                "switch-away backfill must not persist the managed access token into provider storage"
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_switch_adopts_outgoing_cli_rotation_before_account_or_key_overwrite() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity("acct-a", "managed-access-a", "user-a")
                    .await
                    .expect("seed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity("acct-b", "managed-access-b", "user-b")
                    .await
                    .expect("seed account B");
            });

            let provider_a = managed_codex_provider("managed-a", "acct-a");
            let provider_b = managed_codex_provider("managed-b", "acct-b");
            let mut third_party = Provider::with_id(
                "third-party".to_string(),
                "Third Party".to_string(),
                json!({
                    "auth": { "OPENAI_API_KEY": "sk-third-party" },
                    "config": r#"model_provider = "third"
[model_providers.third]
name = "Third"
base_url = "https://third.example/v1"
wire_api = "responses"
"#
                }),
                None,
            );
            third_party.category = Some("custom".to_string());
            for provider in [&provider_a, &provider_b, &third_party] {
                state
                    .db
                    .save_provider(&AppType::Codex, provider)
                    .expect("save provider");
            }

            ProviderService::switch(state, AppType::Codex, &provider_a.id)
                .expect("activate managed A");
            let id_token_a = crate::codex_config::test_codex_id_token("user-a");
            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &crate::codex_config::codex_managed_oauth_auth_value(
                    "acct-a",
                    "cli-access-a1",
                    Some(&id_token_a),
                    "cli-refresh-a1",
                    "2099-01-02T00:00:00Z",
                ),
            )
            .expect("rotate account A live auth");

            ProviderService::switch(state, AppType::Codex, &provider_b.id)
                .expect("switch managed A to managed B");
            assert_eq!(
                crate::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-a")
                )
                .as_deref(),
                Some("cli-refresh-a1"),
                "A's CLI generation must be adopted before B overwrites auth.json"
            );
            let live_b: Value =
                read_json_file(&crate::codex_config::get_codex_auth_path()).expect("read B auth");
            assert_eq!(
                live_b.pointer("/tokens/account_id").and_then(Value::as_str),
                Some("acct-b")
            );

            let id_token_b = crate::codex_config::test_codex_id_token("user-b");
            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &crate::codex_config::codex_managed_oauth_auth_value(
                    "acct-b",
                    "cli-access-b1",
                    Some(&id_token_b),
                    "cli-refresh-b1",
                    "2099-01-03T00:00:00Z",
                ),
            )
            .expect("rotate account B live auth");

            ProviderService::switch(state, AppType::Codex, &third_party.id)
                .expect("switch managed B to API-key provider");
            assert_eq!(
                crate::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-b")
                )
                .as_deref(),
                Some("cli-refresh-b1"),
                "B's CLI generation must be adopted before the third-party switch removes auth.json"
            );
            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "third-party switches are config-only: auth.json is removed"
            );
            let live_config = std::fs::read_to_string(crate::codex_config::get_codex_config_path())
                .expect("read third-party config");
            assert!(
                live_config.contains("experimental_bearer_token = \"sk-third-party\""),
                "the third-party key rides in config.toml; got:\n{live_config}"
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_direct_update_adopts_outgoing_cli_rotation_and_commits_target_binding() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity("acct-a", "managed-access-a", "user-a")
                    .await
                    .expect("seed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity("acct-b", "managed-access-b", "user-b")
                    .await
                    .expect("seed account B");
            });

            let provider = managed_codex_provider("managed-official-a", "acct-a");
            state
                .db
                .save_provider(&AppType::Codex, &provider)
                .expect("save managed official provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed account A");
            assert!(
                crate::block_on(state.db.get_live_backup(&AppType::Codex))
                    .expect("read initial live backup")
                    .is_none(),
                "direct update precondition requires no takeover backup"
            );

            let id_token_a = crate::codex_config::test_codex_id_token("user-a");
            write_json_file(
                &crate::codex_config::get_codex_auth_path(),
                &crate::codex_config::codex_managed_oauth_auth_value(
                    "acct-a",
                    "cli-access-a1",
                    Some(&id_token_a),
                    "cli-refresh-a1",
                    "2099-03-01T00:00:00Z",
                ),
            )
            .expect("simulate account A CLI rotation");

            let mut updated = provider.clone();
            updated.name = "OpenAI Official B".to_string();
            updated
                .meta
                .as_mut()
                .and_then(|meta| meta.auth_binding.as_mut())
                .expect("managed binding")
                .account_id = Some("acct-b".to_string());

            ProviderService::update(state, AppType::Codex, None, updated.clone())
                .expect("directly update managed binding from A to B");

            assert_eq!(
                crate::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-a")
                )
                .as_deref(),
                Some("cli-refresh-a1"),
                "direct update must adopt A's CLI generation before overwriting live auth"
            );
            let saved = state
                .db
                .get_provider_by_id(&provider.id, &AppType::Codex)
                .expect("read updated provider")
                .expect("updated provider exists");
            assert_eq!(saved.name, updated.name);
            assert_eq!(
                saved
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
                    .as_deref(),
                Some("acct-b")
            );

            let live_b: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read account B live auth");
            assert_eq!(
                live_b.pointer("/tokens/account_id").and_then(Value::as_str),
                Some("acct-b")
            );
            assert!(
                crate::codex_config::codex_auth_matches_recorded_managed_oauth(&live_b, "acct-b")
                    .expect("check account B marker"),
                "clearing outgoing account A must not remove account B's marker"
            );
            assert!(
                crate::block_on(state.db.get_live_backup(&AppType::Codex))
                    .expect("read live backup after direct update")
                    .is_none(),
                "direct update must not create a takeover backup"
            );
        });
    }

    #[test]
    #[serial]
    fn same_account_managed_codex_update_rejects_equal_timestamp_refresh_conflict() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-access",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed account");
            });

            let provider = managed_codex_provider("managed-same-account", "acct-managed");
            state
                .db
                .save_provider(&AppType::Codex, &provider)
                .expect("save managed provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed provider");

            // Different refresh material at the exact manager generation
            // timestamp is ambiguous at millisecond precision. A same-account
            // update has no outgoing-account guard, so its managed bundle
            // preflight itself must refuse to overwrite this CLI generation.
            crate::block_on(
                state
                    .codex_oauth_manager
                    .test_set_token_updated_at_ms("acct-managed", 1_700_000_000_000),
            );
            let id_token = crate::codex_config::test_codex_id_token("managed-user");
            let cli_live_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "acct-managed",
                "cli-access-r1",
                Some(&id_token),
                "cli-refresh-r1",
                "2023-11-14T22:13:20Z",
            );
            write_json_file(&crate::codex_config::get_codex_auth_path(), &cli_live_auth)
                .expect("seed equal-timestamp CLI generation");

            let mut updated = provider.clone();
            updated.name = "Managed updated".to_string();
            let error = ProviderService::update(state, AppType::Codex, None, updated)
                .expect_err("ambiguous same-account generation must block the live write");
            assert!(
                error
                    .to_string()
                    .contains("无法安全判断 refresh token 新旧"),
                "update should explain the safe-write rejection: {error}"
            );

            let live_after: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read preserved CLI auth");
            assert_eq!(
                live_after, cli_live_auth,
                "same-account managed update must not overwrite ambiguous CLI token material"
            );
            assert_eq!(
                crate::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-managed")
                )
                .as_deref(),
                Some("test-refresh-token"),
                "ambiguous CLI material must not replace the manager generation either"
            );
            assert_eq!(
                state
                    .db
                    .get_provider_by_id(&provider.id, &AppType::Codex)
                    .expect("read provider after rejected update")
                    .expect("provider remains present")
                    .name,
                provider.name,
                "rejected preflight must leave the provider row unchanged"
            );
        });
    }

    #[test]
    #[serial]
    fn switch_away_rejects_legacy_refresh_conflict_on_every_retry() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-legacy",
                        "managed-access",
                        "legacy-user",
                    )
                    .await
                    .expect("seed managed account");
            });

            let managed = managed_codex_provider("managed-legacy", "acct-legacy");
            let mut unbound = Provider::with_id(
                "unbound-official".to_string(),
                "Unbound Official".to_string(),
                json!({
                    "auth": {},
                    "config": ""
                }),
                None,
            );
            unbound.category = Some("official".to_string());
            for provider in [&managed, &unbound] {
                state
                    .db
                    .save_provider(&AppType::Codex, provider)
                    .expect("save provider");
            }
            ProviderService::switch(state, AppType::Codex, &managed.id)
                .expect("activate managed provider");

            crate::block_on(
                state
                    .codex_oauth_manager
                    .test_set_token_updated_at_ms("acct-legacy", 0),
            );
            let id_token = crate::codex_config::test_codex_id_token("legacy-user");
            let cli_live_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "acct-legacy",
                "cli-access-r1",
                Some(&id_token),
                "cli-refresh-r1",
                "2023-11-14T22:13:20Z",
            );
            write_json_file(&crate::codex_config::get_codex_auth_path(), &cli_live_auth)
                .expect("seed CLI generation against legacy manager state");

            for attempt in 1..=2 {
                let error = ProviderService::switch(state, AppType::Codex, &unbound.id)
                    .expect_err("legacy conflict must block every switch-away retry");
                assert!(
                    error
                        .to_string()
                        .contains("无法安全判断 refresh token 新旧"),
                    "attempt {attempt} should remain ambiguous: {error}"
                );
                let live_after: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                    .expect("read preserved CLI auth");
                assert_eq!(
                    live_after, cli_live_auth,
                    "attempt {attempt} must not overwrite or delete the CLI generation"
                );
                assert_eq!(
                    state
                        .db
                        .get_current_provider(&AppType::Codex)
                        .expect("read current provider")
                        .as_deref(),
                    Some(managed.id.as_str()),
                    "attempt {attempt} must not commit the target provider"
                );
            }

            assert_eq!(
                crate::block_on(
                    state
                        .codex_oauth_manager
                        .test_refresh_token_for_account("acct-legacy")
                )
                .as_deref(),
                Some("test-refresh-token"),
                "ambiguous legacy retries must keep manager material unchanged"
            );
        });
    }

    #[test]
    #[serial]
    fn codex_auth_center_remove_and_logout_clear_live_credentials_and_marker() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-access",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed account");
            });
            let provider = managed_codex_provider("managed-auth-center", "acct-managed");
            state
                .db
                .save_provider(&AppType::Codex, &provider)
                .expect("save managed provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed provider");
            assert!(crate::codex_config::get_codex_auth_path().exists());
            assert!(crate::codex_config::codex_managed_oauth_live_auth_marker_exists());

            crate::block_on(state.codex_oauth_manager.remove_account("acct-managed"))
                .expect("remove managed account");
            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "removing the active account must delete its refreshable live auth"
            );
            assert!(
                !crate::codex_config::codex_managed_oauth_live_auth_marker_exists(),
                "removing the active account must delete its marker"
            );
            assert_eq!(
                state
                    .db
                    .get_provider_by_id(&provider.id, &AppType::Codex)
                    .expect("read provider")
                    .and_then(|provider| provider.meta)
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
                    .as_deref(),
                Some("acct-managed"),
                "the binding is retained so re-login with the same account can recover it"
            );

            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-access-2",
                        "managed-user",
                    )
                    .await
                    .expect("re-login managed account");
            });
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("reactivate managed provider after re-login");
            assert!(crate::codex_config::get_codex_auth_path().exists());

            crate::block_on(state.codex_oauth_manager.clear_auth())
                .expect("logout all managed accounts");
            assert!(!crate::codex_config::get_codex_auth_path().exists());
            assert!(!crate::codex_config::codex_managed_oauth_live_auth_marker_exists());
            assert!(crate::block_on(state.codex_oauth_manager.list_accounts()).is_empty());
        });
    }

    #[test]
    #[serial]
    fn deleted_codex_account_can_rebind_or_switch_in_either_mode() {
        for mode in ["direct", "proxy"] {
            for rebind in [true, false] {
                with_test_home(|state, _| {
                    crate::settings::reload_settings().unwrap();
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    let token = crate::codex_config::test_codex_id_token("same-user");
                    runtime
                        .block_on(
                            state
                                .codex_oauth_manager
                                .add_test_account_with_workspace_and_access_token(
                                    "old-local-id",
                                    "workspace",
                                    "old-access",
                                    Some(&token),
                                ),
                        )
                        .unwrap();
                    runtime.block_on(async {
                        let mut config = state.db.get_proxy_config().await.unwrap();
                        config.listen_port = 0;
                        state.db.update_proxy_config(config).await.unwrap();
                    });
                    let current = managed_codex_provider("current", "old-local-id");
                    state.db.save_provider(&AppType::Codex, &current).unwrap();
                    ProviderService::switch(state, AppType::Codex, &current.id).unwrap();
                    if mode == "proxy" {
                        runtime
                            .block_on(crate::mode::controller::enter(
                                state,
                                &AppType::Codex,
                                false,
                            ))
                            .unwrap();
                    }
                    runtime
                        .block_on(remove_codex_account_leaving_dangling_bindings(
                            state,
                            "old-local-id",
                        ))
                        .unwrap();
                    let restarted = (mode == "direct").then(|| AppState::new(state.db.clone()));
                    let state = restarted.as_ref().unwrap_or(state);
                    // Ordinary login creates a new local ID, even for the same user/workspace.
                    runtime
                        .block_on(
                            state
                                .codex_oauth_manager
                                .add_test_account_with_workspace_and_access_token(
                                    "new-local-id",
                                    "workspace",
                                    "new-access",
                                    Some(&token),
                                ),
                        )
                        .unwrap();
                    assert_eq!(
                        ProviderService::managed_codex_oauth_account_id(
                            &state
                                .db
                                .get_provider_by_id("current", &AppType::Codex)
                                .unwrap()
                                .unwrap()
                        )
                        .as_deref(),
                        Some("old-local-id")
                    );

                    // 绑定已失效：直连下进入路由要报错让用户重新绑定，客户端文件不动。
                    if mode == "direct" {
                        let error = runtime
                            .block_on(crate::mode::controller::enter(
                                state,
                                &AppType::Codex,
                                false,
                            ))
                            .unwrap_err();
                        assert!(error.contains("选择账号"), "{error}");
                        assert!(!crate::mode::current::is_proxy(&AppType::Codex));
                        assert!(!state
                            .proxy_service
                            .live_has_proxy_placeholder(&AppType::Codex));
                    }

                    let target = managed_codex_provider(
                        if rebind { "current" } else { "target" },
                        "new-local-id",
                    );
                    if rebind {
                        ProviderService::update(state, AppType::Codex, None, target.clone())
                            .unwrap();
                    } else {
                        state.db.save_provider(&AppType::Codex, &target).unwrap();
                        ProviderService::switch(state, AppType::Codex, &target.id).unwrap();
                    }
                    let auth: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                        .unwrap_or_else(|error| panic!("{mode}, rebind={rebind}: {error}"));
                    assert_eq!(
                        auth["tokens"]["access_token"], "new-access",
                        "{mode}, rebind={rebind}"
                    );
                    assert_eq!(
                        crate::mode::current::provider_for(
                            &state.db,
                            &AppType::Codex,
                            crate::mode::current::Purpose::InUse,
                        )
                        .unwrap()
                        .as_deref(),
                        Some(target.id.as_str())
                    );

                    if mode == "proxy" && !rebind {
                        // 路由换了，直连指针仍是失效的那家：在路由模式下改它的绑定只存行，
                        // 退出路由时再写回。
                        ProviderService::update(
                            state,
                            AppType::Codex,
                            None,
                            managed_codex_provider("current", "new-local-id"),
                        )
                        .unwrap();
                    }
                    runtime.block_on(async {
                        if mode == "direct" {
                            crate::mode::controller::enter(state, &AppType::Codex, false)
                                .await
                                .unwrap();
                        }
                        crate::mode::controller::exit(state, &AppType::Codex)
                            .await
                            .unwrap();
                        assert!(!state.proxy_service.is_running().await);
                    });
                    let restored: Value =
                        read_json_file(&crate::codex_config::get_codex_auth_path()).unwrap();
                    assert_eq!(
                        restored["tokens"]["access_token"], "new-access",
                        "leaving routing mode must keep the replacement account ({mode}, rebind={rebind})"
                    );
                });
            }
        }
    }

    /// 删号但不解绑供应商：模拟旧版删号留下的悬空行。解绑失败、别的设备同步来的行
    /// 同样会悬空，恢复路径仍要能处理。
    async fn remove_codex_account_leaving_dangling_bindings(
        state: &AppState,
        account_id: &str,
    ) -> Result<(), String> {
        let _switch_guard = state.operations.lock_for_app(AppType::Codex.as_str()).await;
        state
            .codex_oauth_manager
            .remove_account(account_id)
            .await
            .map_err(|error| error.to_string())
    }

    fn seed_codex_recovery_accounts(state: &AppState) {
        crate::block_on(async {
            for (id, user) in [("acct-old", "old-user"), ("acct-new", "new-user")] {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(id, "access", user)
                    .await
                    .expect("seed account");
            }
        });
    }

    #[test]
    #[serial]
    fn codex_account_removal_and_logout_unbind_current_and_inactive_cards() {
        for logout_all in [false, true] {
            with_test_home(|state, _| {
                crate::settings::reload_settings().unwrap();
                seed_codex_recovery_accounts(state);
                let current = managed_codex_provider("current", "acct-old");
                let mut legacy = managed_codex_provider("legacy", "acct-old");
                legacy.category = None;
                legacy.notes = Some("keep notes".into());
                legacy.settings_config["auth"] =
                    json!({"tokens": {"refresh_token": "stale-snapshot"}});
                legacy.settings_config["model"] = json!("keep-model");
                let other = managed_codex_provider("other", "acct-new");
                // 从另一台设备同步来的行：绑的是那台设备的本地账号，本机删号或登出都不能动它，
                // 否则解绑会随云同步回去，弄坏那台设备上正常的绑定。
                let foreign = managed_codex_provider("foreign", "acct-other-device");
                let mut unrelated = managed_codex_provider("unrelated", "acct-old");
                unrelated
                    .meta
                    .as_mut()
                    .unwrap()
                    .auth_binding
                    .as_mut()
                    .unwrap()
                    .source = AuthBindingSource::ProviderConfig;
                for provider in [&current, &legacy, &other, &foreign, &unrelated] {
                    state.db.save_provider(&AppType::Codex, provider).unwrap();
                }
                state.db.save_provider(&AppType::Claude, &current).unwrap();
                state
                    .db
                    .conn
                    .lock()
                    .unwrap()
                    .execute(
                        "UPDATE providers SET meta = json_set(meta, '$.futureField', 'keep') WHERE id = 'legacy'",
                        [],
                    )
                    .unwrap();
                ProviderService::switch(state, AppType::Codex, &current.id).unwrap();
                if logout_all {
                    crate::block_on(
                        crate::services::provider::codex_oauth_switch::logout_codex_oauth_with_switch_lock(state),
                    )
                    .unwrap();
                } else {
                    crate::block_on(
                        crate::services::provider::codex_oauth_switch::remove_codex_oauth_account_with_switch_lock(
                            state, "acct-old",
                        ),
                    )
                    .unwrap();
                }
                for id in ["current", "legacy"] {
                    let saved = state
                        .db
                        .get_provider_by_id(id, &AppType::Codex)
                        .unwrap()
                        .unwrap();
                    assert!(saved.meta.unwrap().auth_binding.is_none());
                    assert_eq!(saved.settings_config["auth"], json!({}));
                    assert_eq!(saved.category.as_deref(), Some("official"));
                }
                let saved_legacy = state
                    .db
                    .get_provider_by_id("legacy", &AppType::Codex)
                    .unwrap()
                    .unwrap();
                assert_eq!(saved_legacy.notes, legacy.notes);
                assert_eq!(saved_legacy.settings_config["model"], "keep-model");
                let future: String = state
                    .db
                    .conn
                    .lock()
                    .unwrap()
                    .query_row(
                        "SELECT json_extract(meta, '$.futureField') FROM providers WHERE id = 'legacy'",
                        [],
                        |row| row.get(0),
                    )
                    .unwrap();
                assert_eq!(future, "keep");
                let bound_to = |id: &str, app: AppType| {
                    state
                        .db
                        .get_provider_by_id(id, &app)
                        .unwrap()
                        .unwrap()
                        .meta
                        .unwrap()
                        .managed_account_id_for("codex_oauth")
                };
                assert_eq!(bound_to("other", AppType::Codex).is_none(), logout_all);
                assert_eq!(
                    bound_to("foreign", AppType::Codex).as_deref(),
                    Some("acct-other-device")
                );
                assert!(state
                    .db
                    .get_provider_by_id("unrelated", &AppType::Codex)
                    .unwrap()
                    .unwrap()
                    .meta
                    .unwrap()
                    .auth_binding
                    .is_some());
                assert!(state
                    .db
                    .get_provider_by_id("current", &AppType::Claude)
                    .unwrap()
                    .unwrap()
                    .meta
                    .unwrap()
                    .auth_binding
                    .is_some());
                assert_eq!(
                    crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                    Some("current")
                );
                assert!(!crate::codex_config::get_codex_auth_path().exists());
                assert!(!crate::codex_config::codex_managed_oauth_live_auth_marker_exists());
                ProviderService::switch(state, AppType::Codex, "legacy").unwrap();
                assert!(
                    !crate::codex_config::get_codex_auth_path().exists(),
                    "unbound legacy card must not resurrect stored credentials"
                );
                assert!(state
                    .db
                    .unbind_codex_managed_accounts(&["acct-old".to_string()])
                    .unwrap()
                    .is_empty());
            });
        }
    }

    #[test]
    #[serial]
    fn codex_failed_batch_unbinding_rolls_back_and_leaves_a_recovery_path() {
        for rebind in [false, true] {
            with_test_home(|state, _| {
                crate::settings::reload_settings().unwrap();
                seed_codex_recovery_accounts(state);
                let current = managed_codex_provider("a-current", "acct-old");
                let second = managed_codex_provider("z-second", "acct-old");
                let target = managed_codex_provider("target", "acct-new");
                for provider in [&current, &second, &target] {
                    state.db.save_provider(&AppType::Codex, provider).unwrap();
                }
                ProviderService::switch(state, AppType::Codex, &current.id).unwrap();
                state
                    .db
                    .conn
                    .lock()
                    .unwrap()
                    .execute_batch(
                        "CREATE TRIGGER reject_unbind BEFORE UPDATE OF meta ON providers
                     WHEN OLD.id = 'z-second' AND json_extract(NEW.meta, '$.authBinding') IS NULL
                     BEGIN SELECT RAISE(FAIL, 'injected unbind failure'); END;",
                    )
                    .unwrap();
                let error = crate::block_on(
                    crate::services::provider::codex_oauth_switch::remove_codex_oauth_account_with_switch_lock(state, "acct-old"),
                )
                .unwrap_err();
                assert!(error.contains("账号已删除") && error.contains("解绑失败"));
                for id in [&current.id, &second.id] {
                    assert_eq!(
                        state
                            .db
                            .get_provider_by_id(id, &AppType::Codex)
                            .unwrap()
                            .unwrap()
                            .meta
                            .unwrap()
                            .managed_account_id_for("codex_oauth")
                            .as_deref(),
                        Some("acct-old")
                    );
                }
                assert!(!crate::block_on(state.codex_oauth_manager.list_accounts())
                    .iter()
                    .any(|account| account.id == "acct-old"));
                if rebind {
                    let replacement = managed_codex_provider(&current.id, "acct-new");
                    ProviderService::update(state, AppType::Codex, None, replacement).unwrap();
                } else {
                    ProviderService::switch(state, AppType::Codex, &target.id).unwrap();
                }
                let auth: Value =
                    read_json_file(&crate::codex_config::get_codex_auth_path()).unwrap();
                assert_eq!(auth["tokens"]["account_id"], "acct-new");
            });
        }
    }

    #[test]
    #[serial]
    fn deleted_codex_account_recovers_after_persisted_startup() {
        for exit_state in ["detached", "crashed"] {
            let _guard = test_guard();
            let _home = TempHome::new();
            crate::settings::reload_settings().unwrap();
            let runtime = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            let db = Arc::new(Database::memory().unwrap());
            let state = AppState::new(db.clone());
            let token = crate::codex_config::test_codex_id_token("same-user");
            runtime.block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_workspace_and_access_token(
                        "old-local-id",
                        "workspace",
                        "old-access",
                        Some(&token),
                    )
                    .await
                    .unwrap();
                let mut config = state.db.get_proxy_config().await.unwrap();
                config.listen_port = 0;
                state.db.update_proxy_config(config).await.unwrap();
            });
            let current = managed_codex_provider("current", "old-local-id");
            state.db.save_provider(&AppType::Codex, &current).unwrap();
            ProviderService::switch(&state, AppType::Codex, "current").unwrap();
            runtime.block_on(async {
                crate::mode::controller::enter(&state, &AppType::Codex, false)
                    .await
                    .unwrap();
                remove_codex_account_leaving_dangling_bindings(&state, "old-local-id")
                    .await
                    .unwrap();
                state
                    .codex_oauth_manager
                    .add_test_account_with_workspace_and_access_token(
                        "new-local-id",
                        "workspace",
                        "new-access",
                        Some(&token),
                    )
                    .await
                    .unwrap();
            });
            // Model a later CLI login. Startup recovery must leave it intact.
            let native_auth = crate::codex_config::codex_managed_oauth_auth_value(
                "native-workspace",
                "native-access",
                Some(&token),
                "native-refresh",
                "2026-09-14T00:00:00Z",
            );
            write_json_file(&crate::codex_config::get_codex_auth_path(), &native_auth).unwrap();
            runtime.block_on(async {
                if exit_state == "detached" {
                    // 直连供应商的账号没了写不出来：只清掉占位符，登录不动。
                    crate::mode::controller::detach_all(&state).await;
                    assert!(!state
                        .proxy_service
                        .live_has_proxy_placeholder(&AppType::Codex));
                } else {
                    // 崩溃：客户端仍指着代理，只是监听没了。
                    state.proxy_service.stop().await.unwrap();
                    assert!(state
                        .proxy_service
                        .live_has_proxy_placeholder(&AppType::Codex));
                }
                assert!(crate::mode::current::is_proxy(&AppType::Codex));
            });
            drop(state);

            crate::settings::reload_settings().unwrap();
            // 内存库跨重启沿用同一实例；要验证的持久化状态（账号库、live-state、
            // settings、auth.json）都在磁盘上。
            let restarted = AppState::new(db);
            assert_eq!(
                ProviderService::managed_codex_oauth_account_id(
                    &restarted
                        .db
                        .get_provider_by_id("current", &AppType::Codex)
                        .unwrap()
                        .unwrap()
                )
                .as_deref(),
                Some("old-local-id"),
            );
            runtime.block_on(async {
                let accounts = restarted.codex_oauth_manager.list_accounts().await;
                assert_eq!(accounts.len(), 1);
                assert_eq!(accounts[0].id, "new-local-id");
                let store_path = crate::config::get_app_config_dir().join("codex_oauth_auth.json");
                let persisted_accounts = fs::read(&store_path).unwrap();
                // Match setup's ordering: extract common config from the direct live
                // file before re-attaching routing mode.
                crate::initialize_common_config_snippets(&restarted);
                crate::mode::controller::startup(&restarted).await;
                // 账号失效时解除客户端接管，保留用户选定的代理模式供修复后重试。
                assert!(
                    crate::mode::current::is_proxy(&AppType::Codex),
                    "{exit_state}"
                );
                assert!(!crate::mode::current::mode_state(&AppType::Codex).attached);
                assert!(
                    restarted
                        .db
                        .get_proxy_config_for_app(&AppType::Codex)
                        .await
                        .unwrap()
                        .enabled
                );
                assert!(restarted.proxy_service.is_running().await);
                assert!(crate::mode::controller::take_startup_attach_failures()
                    .iter()
                    .any(|failure| failure.app_type == AppType::Codex.as_str()));
                assert!(!restarted
                    .proxy_service
                    .live_has_proxy_placeholder(&AppType::Codex));
                assert_eq!(fs::read(&store_path).unwrap(), persisted_accounts);
                assert_eq!(
                    read_json_file::<Value>(&crate::codex_config::get_codex_auth_path()).unwrap(),
                    native_auth
                );
                // Stub the network refresh only after proving the account survived on disk.
                restarted
                    .codex_oauth_manager
                    .test_cache_access_token("new-local-id", "new-access")
                    .await;
            });
            let target = managed_codex_provider("current", "new-local-id");
            ProviderService::update(&restarted, AppType::Codex, None, target).unwrap();
            runtime.block_on(async {
                crate::mode::controller::enter(&restarted, &AppType::Codex, false)
                    .await
                    .unwrap();
                assert!(
                    restarted
                        .db
                        .get_proxy_config_for_app(&AppType::Codex)
                        .await
                        .unwrap()
                        .enabled
                );
                assert!(restarted
                    .proxy_service
                    .live_has_proxy_placeholder(&AppType::Codex));
                crate::mode::controller::exit(&restarted, &AppType::Codex)
                    .await
                    .unwrap();
                assert!(!restarted.proxy_service.is_running().await);
            });
            let auth: Value = read_json_file(&crate::codex_config::get_codex_auth_path()).unwrap();
            assert_eq!(auth["tokens"]["access_token"], "new-access", "{exit_state}");
        }
    }

    #[test]
    #[serial]
    fn codex_auth_center_removal_waits_for_provider_switch_lock() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-access",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed account");
            });

            let switch_guard =
                crate::block_on(state.operations.lock_for_app(AppType::Codex.as_str()));
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    started_tx.send(()).expect("signal removal start");
                    let result = crate::block_on(
                        crate::services::provider::codex_oauth_switch::remove_codex_oauth_account_with_switch_lock(
                            state,
                            "acct-managed",
                        ),
                    );
                    done_tx.send(result).expect("send removal result");
                });
                started_rx.recv().expect("wait for removal task");
                assert!(
                    done_rx
                        .recv_timeout(std::time::Duration::from_millis(100))
                        .is_err(),
                    "Auth Center removal must wait while a provider transaction owns the Codex lock"
                );
                drop(switch_guard);
                done_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("removal should finish after lock release")
                    .expect("remove managed account");
            });
            assert!(crate::block_on(state.codex_oauth_manager.list_accounts()).is_empty());
        });
    }

    #[test]
    #[serial]
    fn codex_auth_center_logout_waits_for_provider_switch_lock() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-access",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed account");
            });
            let provider = managed_codex_provider("managed-logout-lock", "acct-managed");
            state
                .db
                .save_provider(&AppType::Codex, &provider)
                .expect("save managed provider");
            ProviderService::switch(state, AppType::Codex, &provider.id)
                .expect("activate managed provider");
            assert!(crate::codex_config::get_codex_auth_path().exists());
            assert!(crate::codex_config::codex_managed_oauth_live_auth_marker_exists());

            let switch_guard =
                crate::block_on(state.operations.lock_for_app(AppType::Codex.as_str()));
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (done_tx, done_rx) = std::sync::mpsc::channel();
            std::thread::scope(|scope| {
                scope.spawn(|| {
                    started_tx.send(()).expect("signal logout start");
                    let result = crate::block_on(
                        crate::services::provider::codex_oauth_switch::logout_codex_oauth_with_switch_lock(state),
                    );
                    done_tx.send(result).expect("send logout result");
                });
                started_rx.recv().expect("wait for logout task");
                assert!(
                    done_rx
                        .recv_timeout(std::time::Duration::from_millis(100))
                        .is_err(),
                    "Auth Center logout must wait while a provider transaction owns the Codex lock"
                );
                drop(switch_guard);
                done_rx
                    .recv_timeout(std::time::Duration::from_secs(5))
                    .expect("logout should finish after lock release")
                    .expect("logout managed accounts");
            });

            assert!(crate::block_on(state.codex_oauth_manager.list_accounts()).is_empty());
            assert!(
                !crate::codex_config::get_codex_auth_path().exists(),
                "logout must clear the active managed live auth"
            );
            assert!(
                !crate::codex_config::codex_managed_oauth_live_auth_marker_exists(),
                "logout must clear the managed live auth marker"
            );
        });
    }

    #[test]
    #[serial]
    fn switch_to_managed_codex_official_with_unresolvable_account_keeps_current_unchanged() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");

            // 基线：一个普通第三方 provider，可正常切换，作为初始 current。
            // config 必须带自定义 provider 表：config-only 切换要求 key 有
            // provider 级落点。
            let mut baseline = Provider::with_id(
                "baseline".to_string(),
                "Baseline".to_string(),
                json!({
                    "auth": { "OPENAI_API_KEY": "sk-baseline" },
                    "config": "model_provider = \"baseline\"\n[model_providers.baseline]\nbase_url = \"https://baseline.example/v1\"\n"
                }),
                None,
            );
            baseline.category = Some("custom".to_string());

            // 托管 official provider，绑定一个 manager 中不存在的账号：切换预检
            // 取 token 必然失败。
            let mut managed = Provider::with_id(
                "managed-official".to_string(),
                "Managed Official".to_string(),
                json!({ "auth": {}, "config": "" }),
                None,
            );
            managed.category = Some("official".to_string());
            managed.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-missing".to_string()),
                }),
                ..Default::default()
            });

            state
                .db
                .save_provider(&AppType::Codex, &baseline)
                .expect("save baseline");
            state
                .db
                .save_provider(&AppType::Codex, &managed)
                .expect("save managed");

            ProviderService::switch(state, AppType::Codex, "baseline").expect("switch to baseline");

            // 切到绑定了不存在账号的托管 provider：预检失败 → 返回 Err。
            let result = ProviderService::switch(state, AppType::Codex, "managed-official");
            assert!(
                result.is_err(),
                "switch must fail when the managed OAuth token cannot be resolved"
            );

            // current 必须仍是 baseline：预检在提交 current 之前失败，不留下
            // 「DB/UI 指向新 provider、但 live 仍是旧 provider」的不一致状态。
            let current =
                crate::settings::get_effective_current_provider(&state.db, &AppType::Codex)
                    .expect("read current");
            assert_eq!(
                current.as_deref(),
                Some("baseline"),
                "a failed managed switch must not move current off the previous provider"
            );
        });
    }

    #[test]
    fn native_opencode_common_config_excludes_provider_credentials() {
        let config = json!({
            "settings": {"baseURL": "https://native.example", "apiKey": "test", "timeout": 1000},
            "body": {"metadata": {"keep": true}}
        });
        let common: Value = serde_json::from_str(
            &ProviderService::extract_opencode_common_config(&config).unwrap(),
        )
        .unwrap();
        assert_eq!(
            common,
            json!({"settings": {"timeout": 1000}, "body": {"metadata": {"keep": true}}})
        );
    }

    #[test]
    #[serial]
    fn native_opencode_provider_keeps_source_format_through_database_and_sync() {
        use crate::provider::OpenCodeConfigFormat;
        for native in [
            json!({}),
            json!({"models": {"alias": {"modelID": "upstream", "limit": {"input": 10000}}}}),
            json!({
                "package": "@opencode/ai/providers/anthropic",
                "settings": {"baseURL": "https://native.example", "apiKey": "test"},
                "headers": {"X-Tenant": "example"},
                "body": {"metadata": {"keep": true}},
                "models": {"model": {"capabilities": {"tools": true, "input": ["text"], "output": ["text"]}, "variants": [
                    {"id": "low", "settings": {"reasoningEffort": "low"}},
                    {"id": "high", "settings": {"reasoningEffort": "high"}}
                ]}}
            }),
        ] {
            with_test_home(|state, _| {
                // Also exercise upgrading an existing DB entry with no source metadata.
                let existing = Provider::with_id(
                    "anthropic".into(),
                    "Existing name".into(),
                    native.clone(),
                    None,
                );
                state
                    .db
                    .save_provider(&AppType::OpenCode, &existing)
                    .unwrap();
                crate::opencode_config::set_provider_with_format(
                    "anthropic",
                    native.clone(),
                    OpenCodeConfigFormat::V2,
                )
                .unwrap();
                assert_eq!(import_opencode_providers_from_live(state).unwrap(), 1);
                assert_eq!(import_opencode_providers_from_live(state).unwrap(), 0);
                let mut saved = state
                    .db
                    .get_provider_by_id("anthropic", &AppType::OpenCode)
                    .unwrap()
                    .unwrap();
                assert_eq!(saved.name, "Existing name");
                assert_eq!(saved.settings_config, native);
                assert_eq!(
                    saved.meta.as_ref().unwrap().opencode_config_format,
                    Some(OpenCodeConfigFormat::V2)
                );

                // Source metadata must survive removal and re-enabling even for {}.
                crate::opencode_config::remove_provider("anthropic").unwrap();
                live::write_live_snapshot(&AppType::OpenCode, &saved).unwrap();
                assert_eq!(
                    crate::opencode_config::read_opencode_config().unwrap()["providers"]
                        ["anthropic"],
                    native
                );
                saved.settings_config["name"] = json!("Edited native provider");
                let expected = saved.settings_config.clone();
                ProviderService::update(state, AppType::OpenCode, None, saved).unwrap();
                let config = crate::opencode_config::read_opencode_config().unwrap();
                assert_eq!(config["providers"]["anthropic"], expected);
                assert!(config.get("provider").is_none());

                let mut full = Provider::with_id(
                    "anthropic".into(),
                    "Full config".into(),
                    json!({"providers": {"anthropic": expected}}),
                    None,
                );
                live::write_live_snapshot(&AppType::OpenCode, &full).unwrap();
                let before =
                    std::fs::read(crate::opencode_config::get_opencode_config_path().unwrap())
                        .unwrap();
                full.settings_config = json!({"providers": {"other": {}}});
                assert!(live::write_live_snapshot(&AppType::OpenCode, &full).is_err());
                assert_eq!(
                    std::fs::read(crate::opencode_config::get_opencode_config_path().unwrap())
                        .unwrap(),
                    before
                );
            });
        }
    }

    #[test]
    #[serial]
    fn malformed_native_opencode_provider_allows_legacy_import_update_and_sync() {
        use crate::provider::OpenCodeConfigFormat;
        for native in [
            json!({"package": false}),
            json!({"models": {"m": {"variants": {}}}}),
            json!({"settings": {"timeout": "1000"}}),
            json!({"models": {"m": {"limit": {"input": "1000"}}}}),
            json!({"models": {"m": {"capabilities": {"tools": true}}}}),
        ] {
            for existing in [false, true] {
                with_test_home(|state, _| {
                    let provider = opencode_provider("shared");
                    if existing {
                        state
                            .db
                            .save_provider(&AppType::OpenCode, &provider)
                            .unwrap();
                    }
                    let mut expected = json!({
                        "provider": {"shared": provider.settings_config},
                        "providers": {"shared": native},
                        "model": "shared/gpt-4o"
                    });
                    write_json_file(
                        &crate::opencode_config::get_opencode_config_path().unwrap(),
                        &expected,
                    )
                    .unwrap();
                    import_opencode_providers_from_live(state).unwrap();
                    let mut saved = state
                        .db
                        .get_provider_by_id("shared", &AppType::OpenCode)
                        .unwrap()
                        .unwrap();
                    assert_eq!(saved.settings_config, provider.settings_config, "{native}");
                    assert_ne!(
                        saved
                            .meta
                            .as_ref()
                            .and_then(|meta| meta.opencode_config_format),
                        Some(OpenCodeConfigFormat::V2)
                    );
                    assert_eq!(import_opencode_providers_from_live(state).unwrap(), 0);

                    saved.settings_config["options"]["apiKey"] = json!("fake-new");
                    ProviderService::update(state, AppType::OpenCode, None, saved.clone()).unwrap();
                    expected["provider"]["shared"] = saved.settings_config.clone();
                    assert_eq!(
                        crate::opencode_config::read_opencode_config().unwrap(),
                        expected
                    );
                    assert_eq!(
                        state
                            .db
                            .get_provider_by_id("shared", &AppType::OpenCode)
                            .unwrap()
                            .unwrap()
                            .settings_config,
                        saved.settings_config
                    );

                    saved.settings_config["options"]["apiKey"] = json!("fake-synced");
                    state.db.save_provider(&AppType::OpenCode, &saved).unwrap();
                    ProviderService::sync_current_provider_for_app(state, AppType::OpenCode)
                        .unwrap();
                    expected["provider"]["shared"] = saved.settings_config;
                    assert_eq!(
                        crate::opencode_config::read_opencode_config().unwrap(),
                        expected
                    );
                    assert_eq!(import_opencode_providers_from_live(state).unwrap(), 0);
                });
            }
        }
    }

    #[test]
    #[serial]
    fn invalid_native_opencode_edit_is_rejected_before_saving() {
        use crate::provider::OpenCodeConfigFormat;
        with_test_home(|state, _| {
            let native = json!({"settings": {"baseURL": "https://a.example"}});
            crate::opencode_config::set_provider_with_format(
                "native",
                native.clone(),
                OpenCodeConfigFormat::V2,
            )
            .unwrap();
            import_opencode_providers_from_live(state).unwrap();
            let mut edited = state
                .db
                .get_provider_by_id("native", &AppType::OpenCode)
                .unwrap()
                .unwrap();
            // OpenCode requires tools, input and output together.
            edited.settings_config = json!({"settings": {"baseURL": "https://b.example"}, "models": {"m": {"capabilities": {"tools": true}}}});

            let err = ProviderService::update(state, AppType::OpenCode, None, edited.clone())
                .unwrap_err()
                .to_string();
            assert!(err.contains("models.m.capabilities"), "{err}");
            edited.id = "added".into();
            assert!(ProviderService::add(state, AppType::OpenCode, edited, true).is_err());

            let saved = state
                .db
                .get_provider_by_id("native", &AppType::OpenCode)
                .unwrap()
                .unwrap();
            assert_eq!(saved.settings_config, native);
            assert!(state
                .db
                .get_provider_by_id("added", &AppType::OpenCode)
                .unwrap()
                .is_none());
            assert_eq!(
                crate::opencode_config::read_opencode_config().unwrap()["providers"]["native"],
                native
            );
        });
    }

    #[test]
    #[serial]
    fn full_opencode_config_writes_legacy_entry_behind_invalid_native() {
        with_test_home(|_, _| {
            let legacy =
                json!({"npm": "@ai-sdk/openai", "options": {"apiKey": "v1"}, "models": {"m": {}}});
            let full = Provider::with_id(
                "shared".into(),
                "Full config".into(),
                json!({"provider": {"shared": legacy}, "providers": {"shared": {"package": false}}}),
                None,
            );
            live::write_live_snapshot(&AppType::OpenCode, &full).unwrap();
            let config = crate::opencode_config::read_opencode_config().unwrap();
            assert_eq!(config["provider"]["shared"], legacy);
            assert!(config.get("providers").is_none());
        });
    }

    #[test]
    #[serial]
    fn opencode_provider_roundtrip_preserves_fields_on_import_and_update() {
        with_test_home(|state, _| {
            let mut provider = opencode_provider("roundtrip-opencode");
            provider.settings_config["api"] = json!("https://api.example.com/v1");
            provider.settings_config["env"] = json!(["EXAMPLE_API_KEY"]);
            provider.settings_config["models"]["gpt-4o"]["limit"] = json!({
                "input": 120000,
                "context": 128000,
                "output": 8000
            });
            crate::opencode_config::set_provider(&provider.id, provider.settings_config.clone())
                .expect("seed opencode live provider");

            assert_eq!(import_opencode_providers_from_live(state).unwrap(), 1);
            let mut saved = state
                .db
                .get_provider_by_id(&provider.id, &AppType::OpenCode)
                .unwrap()
                .expect("imported provider");
            assert_eq!(saved.settings_config, provider.settings_config);
            assert_eq!(import_opencode_providers_from_live(state).unwrap(), 0);

            saved.settings_config["options"]["apiKey"] = json!("updated-key");
            let expected = saved.settings_config.clone();
            ProviderService::update(state, AppType::OpenCode, None, saved)
                .expect("update imported provider");
            let live = crate::opencode_config::get_providers().unwrap();
            assert_eq!(live[&provider.id], expected);
        });
    }

    #[test]
    #[serial]
    fn opencode_provider_roundtrip_preserves_fields_on_live_write() {
        with_test_home(|_, _| {
            let mut provider = opencode_provider("write-opencode");
            provider.settings_config["api"] = json!("https://api.example.com/v1");
            provider.settings_config["env"] = json!(["EXAMPLE_API_KEY"]);
            provider.settings_config["models"]["gpt-4o"]["limit"] = json!({
                "input": 120000,
                "context": 128000,
                "output": 8000
            });
            let expected = provider.settings_config.clone();

            // Exercise the full-config fragment extraction as well as the writer.
            provider.settings_config = json!({
                "$schema": "https://opencode.ai/config.json",
                "provider": { provider.id.clone(): expected.clone() }
            });
            live::write_live_snapshot(&AppType::OpenCode, &provider)
                .expect("write opencode provider");

            let live = crate::opencode_config::get_providers().unwrap();
            assert_eq!(live[&provider.id], expected);
        });
    }

    #[test]
    #[serial]
    fn import_opencode_providers_from_live_marks_provider_as_live_managed() {
        with_test_home(|state, _| {
            let provider = opencode_provider("imported-opencode");
            crate::opencode_config::set_provider(&provider.id, provider.settings_config.clone())
                .expect("seed opencode live provider");

            let imported = import_opencode_providers_from_live(state)
                .expect("import opencode providers from live");
            assert_eq!(imported, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, &AppType::OpenCode)
                .expect("query imported opencode provider")
                .expect("imported opencode provider should exist");
            assert_eq!(
                saved
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.live_config_managed),
                Some(true),
                "providers imported from live should be treated as live-managed"
            );
        });
    }

    #[test]
    #[serial]
    fn import_opencode_providers_from_live_updates_existing_provider_from_live() {
        with_test_home(|state, _| {
            let provider = opencode_provider("existing-opencode");
            state
                .db
                .save_provider(&AppType::OpenCode, &provider)
                .expect("seed existing opencode provider");

            let mut live_settings = provider.settings_config.clone();
            live_settings.as_object_mut().unwrap().remove("name");
            live_settings["npm"] = Value::String("@ai-sdk/anthropic".to_string());
            live_settings["models"]["gpt-4o"]["name"] = Value::String("Claude Sonnet".to_string());
            crate::opencode_config::set_provider(&provider.id, live_settings)
                .expect("seed edited live opencode provider");

            let updated = import_opencode_providers_from_live(state)
                .expect("import opencode providers from live");
            assert_eq!(updated, 1);

            let saved = state
                .db
                .get_provider_by_id(&provider.id, &AppType::OpenCode)
                .expect("query updated opencode provider")
                .expect("opencode provider should exist");
            assert_eq!(saved.name, provider.name);
            assert_eq!(saved.settings_config["npm"], json!("@ai-sdk/anthropic"));
            assert_eq!(
                saved.settings_config["models"]["gpt-4o"]["name"],
                json!("Claude Sonnet")
            );
        });
    }

    #[test]
    #[serial]
    fn update_persists_non_current_omo_variants_in_database() {
        with_test_home(|state, _| {
            for category in ["omo", "omo-slim"] {
                let provider = opencode_omo_provider(&format!("{category}-provider"), category);
                state
                    .db
                    .save_provider(&AppType::OpenCode, &provider)
                    .unwrap_or_else(|err| panic!("seed {category} provider: {err}"));

                let mut updated = provider.clone();
                updated.name = format!("Updated {category}");
                updated.settings_config["agents"]["writer"]["model"] =
                    Value::String(format!("{category}-next-model"));

                ProviderService::update(state, AppType::OpenCode, None, updated)
                    .unwrap_or_else(|err| panic!("update {category} provider: {err}"));

                let saved = state
                    .db
                    .get_provider_by_id(&provider.id, &AppType::OpenCode)
                    .unwrap_or_else(|err| panic!("query updated {category} provider: {err}"))
                    .unwrap_or_else(|| panic!("{category} provider should exist"));

                assert_eq!(saved.name, format!("Updated {category}"));
                assert_eq!(
                    saved.settings_config["agents"]["writer"]["model"],
                    Value::String(format!("{category}-next-model")),
                    "{category} updates should persist in the database"
                );
            }
        });
    }

    #[test]
    #[serial]
    fn update_current_omo_variant_rewrites_config_from_saved_provider() {
        with_test_home(|state, home| {
            for category in ["omo", "omo-slim"] {
                let provider = opencode_omo_provider(&format!("{category}-current"), category);
                state
                    .db
                    .save_provider(&AppType::OpenCode, &provider)
                    .unwrap_or_else(|err| panic!("seed current {category} provider: {err}"));
                state
                    .db
                    .set_omo_provider_current(&AppType::OpenCode, &provider.id, category)
                    .unwrap_or_else(|err| panic!("set current {category} provider: {err}"));

                let mut updated = provider.clone();
                updated.name = format!("Current {category} updated");
                updated.settings_config["agents"]["writer"]["model"] =
                    Value::String(format!("{category}-saved-model"));
                updated.settings_config["otherFields"]["theme"] =
                    Value::String(format!("{category}-light"));

                ProviderService::update(state, AppType::OpenCode, None, updated)
                    .unwrap_or_else(|err| panic!("update current {category} provider: {err}"));

                let saved = state
                    .db
                    .get_provider_by_id(&provider.id, &AppType::OpenCode)
                    .unwrap_or_else(|err| panic!("query current {category} provider: {err}"))
                    .unwrap_or_else(|| panic!("current {category} provider should exist"));
                assert_eq!(saved.name, format!("Current {category} updated"));

                let written = fs::read_to_string(omo_config_path(home, category))
                    .unwrap_or_else(|err| panic!("read written {category} config: {err}"));
                let written_json: Value = serde_json::from_str(&written)
                    .unwrap_or_else(|err| panic!("parse written {category} config: {err}"));

                assert_eq!(
                    written_json["agents"]["writer"]["model"],
                    Value::String(format!("{category}-saved-model")),
                    "{category} config should be written from the saved provider state"
                );
                assert_eq!(
                    written_json["theme"],
                    Value::String(format!("{category}-light")),
                    "{category} top-level config should reflect updated otherFields"
                );
            }
        });
    }

    #[test]
    #[serial]
    fn update_current_omo_variant_does_not_persist_database_when_file_write_fails() {
        with_test_home(|state, home| {
            let provider = opencode_omo_provider("omo-current", "omo");
            state
                .db
                .save_provider(&AppType::OpenCode, &provider)
                .unwrap_or_else(|err| panic!("seed current omo provider: {err}"));
            state
                .db
                .set_omo_provider_current(&AppType::OpenCode, &provider.id, "omo")
                .unwrap_or_else(|err| panic!("set current omo provider: {err}"));

            let config_dir = home.join(".config").join("opencode");
            fs::create_dir_all(config_dir.parent().expect("config dir parent"))
                .expect("create .config dir");
            fs::write(&config_dir, "not a directory").expect("block opencode config dir");

            let mut updated = provider.clone();
            updated.name = "Current omo updated".to_string();
            updated.settings_config["agents"]["writer"]["model"] =
                Value::String("omo-saved-model".to_string());

            ProviderService::update(state, AppType::OpenCode, None, updated)
                .expect_err("update should fail when current omo file write fails");

            let saved = state
                .db
                .get_provider_by_id(&provider.id, &AppType::OpenCode)
                .unwrap_or_else(|err| panic!("query current omo provider: {err}"))
                .unwrap_or_else(|| panic!("current omo provider should exist"));

            assert_eq!(saved.name, provider.name);
            assert_eq!(
                saved.settings_config["agents"]["writer"]["model"],
                provider.settings_config["agents"]["writer"]["model"],
                "database should remain unchanged when file write fails"
            );
        });
    }

    #[test]
    #[serial]
    fn update_current_omo_variant_rolls_back_file_when_plugin_sync_fails() {
        with_test_home(|state, home| {
            let provider = opencode_omo_provider("omo-current", "omo");
            state
                .db
                .save_provider(&AppType::OpenCode, &provider)
                .unwrap_or_else(|err| panic!("seed current omo provider: {err}"));
            state
                .db
                .set_omo_provider_current(&AppType::OpenCode, &provider.id, "omo")
                .unwrap_or_else(|err| panic!("set current omo provider: {err}"));

            let config_path = omo_config_path(home, "omo");
            fs::create_dir_all(config_path.parent().expect("omo config parent"))
                .expect("create omo config dir");
            let previous_content = serde_json::to_string_pretty(&json!({
                "theme": "legacy-live-theme",
                "agents": {
                    "writer": {
                        "model": "legacy-live-model"
                    }
                },
                "categories": {
                    "default": ["writer"]
                }
            }))
            .expect("serialize previous config");
            fs::write(&config_path, &previous_content).expect("seed previous omo config");

            let opencode_config_path = home.join(".config").join("opencode").join("opencode.json");
            fs::write(&opencode_config_path, "{ invalid json").expect("seed malformed opencode");

            let mut updated = provider.clone();
            updated.name = "Current omo updated".to_string();
            updated.settings_config["agents"]["writer"]["model"] =
                Value::String("omo-saved-model".to_string());
            updated.settings_config["otherFields"]["theme"] =
                Value::String("omo-light".to_string());

            ProviderService::update(state, AppType::OpenCode, None, updated)
                .expect_err("update should fail when plugin sync fails");

            let saved = state
                .db
                .get_provider_by_id(&provider.id, &AppType::OpenCode)
                .unwrap_or_else(|err| panic!("query current omo provider: {err}"))
                .unwrap_or_else(|| panic!("current omo provider should exist"));

            assert_eq!(saved.name, provider.name);
            assert_eq!(
                saved.settings_config["agents"]["writer"]["model"],
                provider.settings_config["agents"]["writer"]["model"],
                "database should remain unchanged when plugin sync fails"
            );

            let written =
                fs::read_to_string(&config_path).expect("read rolled back omo config content");
            assert_eq!(
                written, previous_content,
                "OMO config should roll back to its previous on-disk contents"
            );
        });
    }

    #[test]
    #[serial]
    fn universal_provider_icon_is_validated_on_save_and_sync() {
        with_test_home(|state, _home| {
            let mut universal = UniversalProvider::new(
                "icon".into(),
                "Icon".into(),
                "custom".into(),
                "https://api.example".into(),
                "key".into(),
            );
            universal.apps.claude = true;
            // 只有文件头、没有像素数据的「PNG」
            universal.icon =
                Some("data:image/png;base64,iVBORw0KGgoAAAANSUhEUgAAAGAAAABgCAYAAAA=".into());
            assert!(ProviderService::upsert_universal(state, universal.clone()).is_err());
            assert!(state.db.get_universal_provider("icon").unwrap().is_none());

            // 绕过保存直接写进库的坏图标，同步到应用时也不会复制给子供应商
            state.db.save_universal_provider(&universal).unwrap();
            assert!(ProviderService::sync_universal_to_apps(state, "icon").is_err());
            assert!(state
                .db
                .get_provider_by_id("universal-claude-icon", &AppType::Claude)
                .unwrap()
                .is_none());
        });
    }

    #[test]
    #[serial]
    fn universal_sync_preserves_claude_api_key_authentication() {
        with_test_home(|state, _home| {
            let mut universal = UniversalProvider::new(
                "api-key-auth".into(),
                "Shared".into(),
                "custom".into(),
                "https://new.example".into(),
                "new-key".into(),
            );
            universal.apps.claude = true;
            let mut child = universal.to_claude_provider().unwrap();
            child.settings_config = json!({"env": {
                "ANTHROPIC_API_KEY": "old-key",
                "KEEP_LOCAL_ENV": "preserved"
            }});
            child.meta = Some(ProviderMeta {
                api_key_field: Some("ANTHROPIC_API_KEY".into()),
                ..Default::default()
            });
            state.db.save_provider(&AppType::Claude, &child).unwrap();
            state.db.save_universal_provider(&universal).unwrap();

            ProviderService::sync_universal_to_apps(state, &universal.id).unwrap();
            let synced = state
                .db
                .get_provider_by_id(&child.id, &AppType::Claude)
                .unwrap()
                .unwrap();
            assert_eq!(
                synced.settings_config["env"]["ANTHROPIC_API_KEY"],
                "new-key"
            );
            assert!(synced.settings_config["env"]
                .get("ANTHROPIC_AUTH_TOKEN")
                .is_none());
            assert_eq!(synced.settings_config["env"]["KEEP_LOCAL_ENV"], "preserved");
            assert_eq!(
                synced.meta.unwrap().api_key_field.as_deref(),
                Some("ANTHROPIC_API_KEY")
            );
        });
    }

    #[test]
    #[serial]
    fn sync_universal_to_apps_preserves_child_metadata() {
        with_test_home(|state, _home| {
            let mut universal = UniversalProvider::new(
                "metadata".into(),
                "Original".into(),
                "custom".into(),
                "https://old.example".into(),
                "old-key".into(),
            );
            universal.apps.claude = true;
            universal.apps.codex = true;

            universal.meta = Some(
                serde_json::from_value(json!({
                    "usage_script": {"enabled": false, "language": "javascript", "code": "parent"}
                }))
                .unwrap(),
            );
            state.db.save_universal_provider(&universal).unwrap();
            ProviderService::sync_universal_to_apps(state, &universal.id).unwrap();

            let mut expected = Vec::new();
            for (index, app) in [AppType::Claude, AppType::Codex].iter().enumerate() {
                let id = format!("universal-{app}-metadata");
                let mut child = state.db.get_provider_by_id(&id, app).unwrap().unwrap();
                assert_eq!(
                    serde_json::to_value(&child.meta).unwrap(),
                    serde_json::to_value(&universal.meta).unwrap()
                );
                child.meta = Some(
                    serde_json::from_value(json!({
                        "usage_script": {"enabled": true, "language": "javascript", "code": app,
                            "apiKey": "usage-only-key", "autoQueryInterval": 15},
                        "commonConfigEnabled": false,
                        "endpointAutoSelect": true
                    }))
                    .unwrap(),
                );
                child.created_at = Some(123 + index as i64);
                child.sort_index = Some(10 + index);
                child.in_failover_queue = true;
                child.settings_config["local_setting"] = json!(app);
                state.db.save_provider(app, &child).unwrap();
                state.db.add_to_failover_queue(app, &id).unwrap();
                state
                    .db
                    .add_custom_endpoint(app, &id, "https://extra.example")
                    .unwrap();
                expected.push(child);
            }

            universal.name = "Updated".into();
            universal.base_url = "https://new.example".into();
            universal.api_key = "new-key".into();
            universal.notes = Some("shared note".into());
            universal.models = serde_json::from_value(json!({
                "claude": {"model": "claude-new"},
                "codex": {"model": "codex-new", "reasoningEffort": "low"},
                "gemini": {"model": "gemini-new"}
            }))
            .unwrap();
            // Both absent and present parent metadata must not overwrite child settings.
            for parent_meta in [None, universal.meta.clone()] {
                universal.meta = parent_meta;
                state.db.save_universal_provider(&universal).unwrap();
                ProviderService::sync_universal_to_apps(state, &universal.id).unwrap();
                for (app, before) in [AppType::Claude, AppType::Codex].iter().zip(&expected) {
                    let after = state
                        .db
                        .get_provider_by_id(&before.id, app)
                        .unwrap()
                        .unwrap();
                    assert_eq!(
                        serde_json::to_value(&after.meta).unwrap(),
                        serde_json::to_value(&before.meta).unwrap(),
                        "{app}"
                    );
                    assert_eq!(after.created_at, before.created_at, "{app}");
                    assert_eq!(after.sort_index, before.sort_index, "{app}");
                    assert_eq!(after.in_failover_queue, before.in_failover_queue, "{app}");
                    assert_eq!(after.name, "Updated");
                    assert_eq!(after.notes.as_deref(), Some("shared note"));
                    assert_eq!(after.settings_config["local_setting"], json!(app));
                    let generated = match app {
                        AppType::Claude => universal.to_claude_provider(),
                        AppType::Codex => universal.to_codex_provider(),
                        _ => unreachable!("only Claude and Codex children are generated"),
                    }
                    .unwrap();
                    let mut expected_settings = before.settings_config.clone();
                    ProviderService::merge_json(&mut expected_settings, &generated.settings_config);
                    assert_eq!(after.settings_config, expected_settings);
                    assert_eq!(
                        state.db.get_all_providers(app).unwrap()[&before.id]
                            .meta
                            .as_ref()
                            .unwrap()
                            .custom_endpoints
                            .len(),
                        1
                    );
                }
            }
        });
    }

    #[test]
    #[serial]
    fn sync_universal_to_apps_reprojects_current_child_to_live() {
        with_test_home(|state, _home| {
            let mut universal = UniversalProvider::new(
                "shared".to_string(),
                "Shared Relay".to_string(),
                "custom".to_string(),
                "https://api.new.example".to_string(),
                "new-key".to_string(),
            );
            universal.apps.claude = true;
            universal.models.claude = Some(ClaudeModelConfig {
                model: Some("claude-sonnet-4".to_string()),
                ..Default::default()
            });
            state
                .db
                .save_universal_provider(&universal)
                .expect("save universal provider");

            let child = universal
                .to_claude_provider()
                .expect("claude child provider");
            state
                .db
                .save_provider(&AppType::Claude, &child)
                .expect("seed child provider");
            state
                .db
                .set_current_provider(&AppType::Claude, &child.id)
                .expect("set current child");
            crate::settings::set_current_provider(&AppType::Claude, Some(&child.id))
                .expect("set local current child");

            let mut old_live = child.settings_config.clone();
            old_live["env"]["ANTHROPIC_BASE_URL"] =
                Value::String("https://api.old.example".to_string());
            write_json_file(&get_claude_settings_path(), &old_live).expect("seed old live");

            ProviderService::sync_universal_to_apps(state, "shared")
                .expect("sync universal provider");

            let live: Value = read_json_file(&get_claude_settings_path()).expect("read live");
            assert_eq!(
                live["env"]["ANTHROPIC_BASE_URL"].as_str(),
                Some("https://api.new.example")
            );
            assert_eq!(
                live["env"]["ANTHROPIC_AUTH_TOKEN"].as_str(),
                Some("new-key")
            );
        });
    }

    #[test]
    #[serial]
    fn add_first_managed_codex_with_missing_account_leaves_no_provider_or_live_state() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            let provider = managed_codex_provider("managed-missing", "acct-missing");
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture empty Codex live state");

            ProviderService::add(state, AppType::Codex, provider.clone(), false)
                .expect_err("missing managed account should fail before add commits");

            assert!(
                state
                    .db
                    .get_provider_by_id(&provider.id, &AppType::Codex)
                    .expect("query failed managed add")
                    .is_none(),
                "failed preflight must not leave an orphan provider row"
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Codex)
                    .expect("read current after failed add"),
                None
            );
            assert_eq!(
                crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture Codex live after failed add"),
                live_before,
                "failed preflight must not mutate Codex live files"
            );
        });
    }

    #[test]
    #[serial]
    fn add_first_managed_codex_current_failure_rolls_forward_on_recovery() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed",
                        "managed-token",
                        "managed-user",
                    )
                    .await
                    .expect("seed managed Codex OAuth account");
            });

            let provider = managed_codex_provider("managed-first", "acct-managed");
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture empty Codex live state");
            {
                let conn = state.db.conn.lock().expect("lock database");
                conn.execute_batch(
                    "CREATE TRIGGER reject_first_managed_current_update
                     BEFORE UPDATE OF is_current ON providers
                     WHEN NEW.app_type = 'codex'
                       AND NEW.id = 'managed-first'
                       AND NEW.is_current = 1
                     BEGIN
                       SELECT RAISE(ABORT, 'forced first managed Codex current failure');
                     END;",
                )
                .expect("install first-current failure trigger");
            }

            let error = ProviderService::add(state, AppType::Codex, provider.clone(), false)
                .expect_err("DB current failure surfaces");
            assert!(
                error
                    .to_string()
                    .contains("forced first managed Codex current failure"),
                "add should surface the DB current failure, got: {error}"
            );
            // 文件已经发布：行留着，pending 等着补完指针，live 不回滚。
            assert!(state
                .db
                .get_provider_by_id(&provider.id, &AppType::Codex)
                .expect("query provider")
                .is_some());
            assert!(crate::mode::operation::has_pending(AppType::Codex.as_str()));
            assert_ne!(
                crate::codex_config::CodexLiveStateSnapshot::capture().expect("capture live"),
                live_before
            );

            state
                .db
                .conn
                .lock()
                .expect("lock database")
                .execute_batch("DROP TRIGGER reject_first_managed_current_update;")
                .expect("drop trigger");
            crate::mode::operation::recover_on_startup(&state.db);
            assert!(!crate::mode::operation::has_pending(
                AppType::Codex.as_str()
            ));
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Codex)
                    .expect("read current after recovery")
                    .as_deref(),
                Some(provider.id.as_str())
            );
        });
    }

    #[test]
    #[serial]
    fn missing_codex_account_preserves_native_login_but_corrupt_store_blocks_recovery() {
        for takeover in [false, true] {
            for corrupt in [false, true] {
                with_test_home(|state, _| {
                    crate::settings::reload_settings().unwrap();
                    crate::settings::update_settings(crate::settings::AppSettings {
                        preserve_codex_official_auth_on_switch: true,
                        ..Default::default()
                    })
                    .unwrap();
                    let runtime = tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .unwrap();
                    runtime
                        .block_on(
                            state
                                .codex_oauth_manager
                                .add_test_account_with_user_identity("old", "access", "user"),
                        )
                        .unwrap();
                    let current = managed_codex_provider("current", "old");
                    state.db.save_provider(&AppType::Codex, &current).unwrap();
                    ProviderService::switch(state, AppType::Codex, "current").unwrap();
                    let mut auth: Value =
                        read_json_file(&crate::codex_config::get_codex_auth_path()).unwrap();
                    if takeover {
                        runtime.block_on(async {
                            // 端口 0 启动后实际端口会记进库，重开的 AppState 不启动代理
                            // 也能算出契约地址。
                            let mut config = state.db.get_proxy_config().await.unwrap();
                            config.listen_port = 0;
                            state.db.update_proxy_config(config).await.unwrap();
                            crate::mode::controller::enter(state, &AppType::Codex, false)
                                .await
                                .unwrap();
                        });
                    }
                    runtime
                        .block_on(remove_codex_account_leaving_dangling_bindings(state, "old"))
                        .unwrap();
                    // A stale marker must not claim a later native login of the same user.
                    auth["tokens"]["refresh_token"] = json!("native-rotated-token");
                    write_json_file(&crate::codex_config::get_codex_auth_path(), &auth).unwrap();
                    crate::codex_config::record_codex_managed_oauth_live_auth(&auth, "old")
                        .unwrap();
                    if corrupt {
                        fs::write(
                            crate::config::get_app_config_dir().join("codex_oauth_auth.json"),
                            "{broken",
                        )
                        .unwrap();
                    }
                    let restarted = AppState::new(state.db.clone());
                    let target = Provider::with_id(
                        "target".into(),
                        "Third party".into(),
                        codex_settings("https://example.test/v1", "sk-target"),
                        None,
                    );
                    state.db.save_provider(&AppType::Codex, &target).unwrap();
                    let before = crate::codex_config::CodexLiveStateSnapshot::capture().unwrap();
                    let result = ProviderService::switch(&restarted, AppType::Codex, "target");
                    if corrupt {
                        assert!(result.is_err());
                        assert_eq!(
                            crate::codex_config::CodexLiveStateSnapshot::capture().unwrap(),
                            before
                        );
                        assert_eq!(
                            crate::mode::current::provider_for(
                                &state.db,
                                &AppType::Codex,
                                crate::mode::current::Purpose::InUse,
                            )
                            .unwrap()
                            .as_deref(),
                            Some("current")
                        );
                    } else {
                        result.unwrap();
                        assert!(
                            !crate::codex_config::codex_managed_oauth_live_auth_marker_exists(),
                            "missing account must relinquish ownership"
                        );
                    }
                    assert_eq!(
                        read_json_file::<Value>(&crate::codex_config::get_codex_auth_path())
                            .unwrap(),
                        auth
                    );
                });
            }
        }
    }

    #[test]
    #[serial]
    fn managed_codex_switch_db_current_failure_rolls_forward_on_recovery() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed-a",
                        "managed-token-a",
                        "user-a",
                    )
                    .await
                    .expect("seed first managed Codex OAuth account");
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed-b",
                        "managed-token-b",
                        "user-b",
                    )
                    .await
                    .expect("seed second managed Codex OAuth account");
            });

            let managed_provider = |id: &str, account_id: &str, model: &str| {
                let mut provider = Provider::with_id(
                    id.to_string(),
                    format!("Managed {id}"),
                    json!({
                        "auth": {},
                        "config": format!("model = \"{model}\"\n"),
                        "modelCatalog": {
                            "models": [{ "model": model }]
                        }
                    }),
                    None,
                );
                provider.category = Some("official".to_string());
                provider.meta = Some(ProviderMeta {
                    auth_binding: Some(AuthBinding {
                        source: AuthBindingSource::ManagedAccount,
                        auth_provider: Some("codex_oauth".to_string()),
                        account_id: Some(account_id.to_string()),
                    }),
                    ..Default::default()
                });
                provider
            };

            let provider_a = managed_provider("managed-a", "acct-managed-a", "gpt-5.4-managed-a");
            let provider_b = managed_provider("managed-b", "acct-managed-b", "gpt-5.4-managed-b");
            state
                .db
                .save_provider(&AppType::Codex, &provider_a)
                .expect("save first managed provider");
            state
                .db
                .save_provider(&AppType::Codex, &provider_b)
                .expect("save second managed provider");

            ProviderService::switch(state, AppType::Codex, &provider_a.id)
                .expect("activate first managed provider");
            let auth_before: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read first managed auth");
            assert!(
                crate::codex_config::get_codex_config_path().exists(),
                "baseline must include config.toml"
            );
            assert!(
                crate::codex_config::get_codex_model_catalog_path().exists(),
                "baseline must include the generated model catalog"
            );
            assert!(
                crate::codex_config::codex_auth_matches_recorded_managed_oauth(
                    &auth_before,
                    "acct-managed-a",
                )
                .expect("check first managed auth marker"),
                "baseline must include a marker owned by the first managed account"
            );
            let live_before = crate::codex_config::CodexLiveStateSnapshot::capture()
                .expect("capture auth/config/catalog/marker before failed switch");

            {
                let conn = state.db.conn.lock().expect("lock database");
                conn.execute_batch(
                    "CREATE TRIGGER reject_managed_b_current_update
                     BEFORE UPDATE OF is_current ON providers
                     WHEN NEW.app_type = 'codex'
                       AND NEW.id = 'managed-b'
                       AND NEW.is_current = 1
                     BEGIN
                       SELECT RAISE(ABORT, 'forced managed Codex current failure');
                     END;",
                )
                .expect("install current-provider failure trigger");
            }

            let error = ProviderService::switch(state, AppType::Codex, &provider_b.id)
                .expect_err("DB current failure should abort managed switch");
            assert!(
                error
                    .to_string()
                    .contains("forced managed Codex current failure"),
                "switch should surface the DB commit failure, got: {error}"
            );

            // 文件已经发布，只是指针没落定：不回滚，pending 等着补完。
            assert_ne!(
                crate::codex_config::CodexLiveStateSnapshot::capture().expect("capture live"),
                live_before
            );
            let auth_after: Value = read_json_file(&crate::codex_config::get_codex_auth_path())
                .expect("read managed auth");
            assert!(
                crate::codex_config::codex_auth_matches_recorded_managed_oauth(
                    &auth_after,
                    "acct-managed-b",
                )
                .expect("check managed marker"),
                "auth.json and the marker belong to the target account"
            );
            assert!(crate::mode::operation::has_pending(AppType::Codex.as_str()));

            state
                .db
                .conn
                .lock()
                .expect("lock database")
                .execute_batch("DROP TRIGGER reject_managed_b_current_update;")
                .expect("drop trigger");
            crate::mode::operation::recover_on_startup(&state.db);
            assert!(!crate::mode::operation::has_pending(
                AppType::Codex.as_str()
            ));
            assert_eq!(
                crate::settings::get_current_provider(&AppType::Codex).as_deref(),
                Some(provider_b.id.as_str())
            );
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Codex)
                    .expect("read DB current after recovery")
                    .as_deref(),
                Some(provider_b.id.as_str())
            );
        });
    }

    #[test]
    #[serial]
    fn managed_codex_update_rechecks_current_after_waiting_for_switch_lock() {
        with_test_home(|state, _| {
            crate::settings::reload_settings().expect("reload settings");
            crate::block_on(async {
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed-a",
                        "managed-token-a",
                        "user-a",
                    )
                    .await
                    .expect("seed managed account A");
                state
                    .codex_oauth_manager
                    .add_test_account_with_user_identity(
                        "acct-managed-b",
                        "managed-token-b",
                        "user-b",
                    )
                    .await
                    .expect("seed managed account B");
            });

            let mut official = Provider::with_id(
                "managed-official-a".to_string(),
                "OpenAI Official".to_string(),
                json!({ "auth": {}, "config": "model = \"gpt-5.4\"\n" }),
                None,
            );
            official.category = Some("official".to_string());
            official.meta = Some(ProviderMeta {
                auth_binding: Some(AuthBinding {
                    source: AuthBindingSource::ManagedAccount,
                    auth_provider: Some("codex_oauth".to_string()),
                    account_id: Some("acct-managed-a".to_string()),
                }),
                ..Default::default()
            });
            state
                .db
                .save_provider(&AppType::Codex, &official)
                .expect("save official A");
            state
                .db
                .set_current_provider(&AppType::Codex, &official.id)
                .expect("set official current");
            crate::settings::set_current_provider(&AppType::Codex, Some(&official.id))
                .expect("set local official current");

            let mut third_party = Provider::with_id(
                "third-party-current".to_string(),
                "Third Party".to_string(),
                json!({
                    "auth": { "OPENAI_API_KEY": "sk-third" },
                    "config": r#"model_provider = "third"
[model_providers.third]
name = "Third"
base_url = "https://third.example/v1"
wire_api = "responses"
"#
                }),
                None,
            );
            third_party.category = Some("custom".to_string());
            state
                .db
                .save_provider(&AppType::Codex, &third_party)
                .expect("save third party");

            let mut updated = official.clone();
            updated
                .meta
                .as_mut()
                .and_then(|meta| meta.auth_binding.as_mut())
                .expect("managed binding")
                .account_id = Some("acct-managed-b".to_string());

            let switch_guard =
                crate::block_on(state.operations.lock_for_app(AppType::Codex.as_str()));
            let (started_tx, started_rx) = std::sync::mpsc::channel();
            let (update_result, live_after_switch) = std::thread::scope(|scope| {
                let updater = scope.spawn(move || {
                    started_tx.send(()).expect("signal updater start");
                    ProviderService::update(state, AppType::Codex, None, updated)
                });
                started_rx.recv().expect("wait for updater");

                // This emulates a switch that already owns the per-app lock and
                // commits a different current target before the queued update is
                // allowed to inspect current/existing state.
                state
                    .db
                    .set_current_provider(&AppType::Codex, &third_party.id)
                    .expect("switch DB current to third party");
                crate::settings::set_current_provider(
                    &AppType::Codex,
                    Some(third_party.id.as_str()),
                )
                .expect("switch local current to third party");
                write_live_for_state(state, &AppType::Codex, &third_party)
                    .expect("write third-party live");
                let live_after_switch = crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture third-party live");

                drop(switch_guard);
                let result = updater.join().expect("join managed updater");
                (result, live_after_switch)
            });

            update_result.expect("save queued non-current managed row");
            assert_eq!(
                state
                    .db
                    .get_current_provider(&AppType::Codex)
                    .expect("read DB current")
                    .as_deref(),
                Some(third_party.id.as_str())
            );
            assert_eq!(
                crate::codex_config::CodexLiveStateSnapshot::capture()
                    .expect("capture live after queued update"),
                live_after_switch,
                "queued provider edit must not rewrite the newly switched current live"
            );
            let saved_official = state
                .db
                .get_provider_by_id(&official.id, &AppType::Codex)
                .expect("read saved official")
                .expect("official exists");
            assert_eq!(
                saved_official
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.managed_account_id_for("codex_oauth")),
                Some("acct-managed-b".to_string())
            );
        });
    }
}

impl ProviderService {
    pub fn managed_codex_oauth_account_id(provider: &Provider) -> Option<String> {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.managed_account_id_for("codex_oauth"))
            .map(|id| id.trim().to_string())
            .filter(|id| !id.is_empty())
    }

    fn normalize_provider_if_claude(app_type: &AppType, provider: &mut Provider) {
        if matches!(app_type, AppType::Claude) {
            let mut v = provider.settings_config.clone();
            if normalize_claude_models_in_value(&mut v) {
                provider.settings_config = v;
            }
        }
    }

    /// Check whether a provider exists in live config, tolerating parse errors
    /// only for providers that are explicitly marked as DB-only.
    fn check_live_config_exists(
        app_type: &AppType,
        provider_id: &str,
        live_config_managed: Option<bool>,
    ) -> Result<bool, AppError> {
        if live_config_managed == Some(false) {
            Ok(provider_exists_in_live_config(app_type, provider_id).unwrap_or(false))
        } else {
            provider_exists_in_live_config(app_type, provider_id)
        }
    }

    fn provider_live_config_managed(provider: &Provider) -> Option<bool> {
        provider
            .meta
            .as_ref()
            .and_then(|meta| meta.live_config_managed)
    }

    fn set_provider_live_config_managed(provider: &mut Provider, managed: bool) {
        provider
            .meta
            .get_or_insert_with(Default::default)
            .live_config_managed = Some(managed);
    }

    fn normalize_usage_script_credential_overrides(app_type: &AppType, provider: &mut Provider) {
        let current_credentials = provider.resolve_usage_credentials(app_type);

        let Some(usage_script) = provider
            .meta
            .as_mut()
            .and_then(|meta| meta.usage_script.as_mut())
        else {
            return;
        };

        if usage_script.template_type.as_deref() == Some("token_plan") {
            return;
        }

        if usage_script.api_key.as_deref().is_some_and(|api_key| {
            Self::should_clear_usage_api_key_override(api_key, &current_credentials)
        }) {
            usage_script.api_key = None;
        }

        if usage_script.base_url.as_deref().is_some_and(|base_url| {
            Self::should_clear_usage_base_url_override(base_url, &current_credentials)
        }) {
            usage_script.base_url = None;
        }
    }

    fn should_clear_usage_api_key_override(
        script_api_key: &str,
        current_credentials: &(String, String),
    ) -> bool {
        let candidate = script_api_key.trim();
        if candidate.is_empty() {
            return true;
        }

        let matches_provider_key = |api_key: &str| {
            let api_key = api_key.trim();
            !api_key.is_empty() && api_key == candidate
        };

        matches_provider_key(&current_credentials.1)
    }

    fn should_clear_usage_base_url_override(
        script_base_url: &str,
        current_credentials: &(String, String),
    ) -> bool {
        let candidate = Self::normalize_usage_base_url_for_compare(script_base_url);
        if candidate.is_empty() {
            return true;
        }

        let matches_provider_base_url = |base_url: &str| {
            let base_url = Self::normalize_usage_base_url_for_compare(base_url);
            !base_url.is_empty() && base_url == candidate
        };

        matches_provider_base_url(&current_credentials.0)
    }

    fn normalize_usage_base_url_for_compare(base_url: &str) -> String {
        base_url.trim().trim_end_matches('/').to_string()
    }

    /// 库里读不出来的供应商：界面单独列出，只能删除。
    pub fn list_unreadable(
        state: &AppState,
        app_type: AppType,
    ) -> Result<Vec<UnreadableEntry>, AppError> {
        Ok(state.db.get_provider_rows(&app_type)?.unreadable)
    }

    /// Get current provider ID
    ///
    /// 使用有效的当前供应商 ID（验证过存在性）。
    /// 优先从本地 settings 读取，验证后 fallback 到数据库的 is_current 字段。
    /// 这确保了云同步场景下多设备可以独立选择供应商，且返回的 ID 一定有效。
    ///
    /// 对于累加模式应用（OpenCode, OpenClaw），不存在"当前供应商"概念，直接返回空字符串。
    pub fn current(state: &AppState, app_type: AppType) -> Result<String, AppError> {
        // Additive mode apps have no "current" provider concept
        if app_type.is_additive_mode() {
            return Ok(String::new());
        }
        // 代理模式下界面上的「当前」是代理路由到的那家。
        crate::mode::current::provider_for(
            &state.db,
            &app_type,
            crate::mode::current::Purpose::InUse,
        )
        .map(|opt| opt.unwrap_or_default())
    }

    fn save_native_provider(
        state: &AppState,
        app_type: &AppType,
        provider: &Provider,
        write_live: bool,
        create: bool,
    ) -> Result<bool, AppError> {
        if matches!(app_type, AppType::Dsh) {
            return if write_live {
                crate::dsh_config::save_and_commit(
                    &provider.id,
                    Some(provider.settings_config.clone()),
                    create,
                    || {
                        state.db.save_provider(app_type, provider)?;
                        Ok(true)
                    },
                )
            } else {
                state.db.save_provider(app_type, provider)?;
                Ok(true)
            };
        }
        Err(AppError::InvalidInput(format!(
            "{} does not use native provider storage",
            app_type.as_str()
        )))
    }

    /// 新增供应商之前的规范化和校验，`add` 和 `add_without_switching` 共用。
    fn prepare_new_provider(app_type: &AppType, provider: &mut Provider) -> Result<(), AppError> {
        // Normalize Claude model keys
        Self::normalize_provider_if_claude(app_type, provider);
        Self::validate_provider_settings(app_type, provider)?;
        Self::normalize_usage_script_credential_overrides(app_type, provider);
        if matches!(app_type, AppType::Claude | AppType::Codex) {
            keep_common_config_for_old_versions(provider);
        }
        Ok(())
    }

    /// 只把供应商存进列表，不写客户端配置、不动当前指针（切换式应用）。
    ///
    /// `add` 在还没有当前供应商时会让新的这家直接生效；深链接导入时用户关掉了「导入后立即
    /// 切换」，就不能这样，交给用户之后自己切换。
    pub fn add_without_switching(
        state: &AppState,
        app_type: AppType,
        provider: Provider,
    ) -> Result<bool, AppError> {
        if app_type.is_additive_mode() {
            return Self::add(state, app_type, provider, false);
        }
        let mut provider = provider;
        Self::prepare_new_provider(&app_type, &mut provider)?;
        // 和切换互斥：等着的切换不能看到只存了一半的行
        let _native_guard = lock_native_provider_operations(state, &app_type);
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;
        EditorSaveKind::Add.validate_target(state, &app_type, &provider.id)?;
        state.db.save_provider(&app_type, &provider)?;
        Ok(true)
    }

    /// Add a new provider
    pub fn add(
        state: &AppState,
        app_type: AppType,
        provider: Provider,
        add_to_live: bool,
    ) -> Result<bool, AppError> {
        let _native_guard = lock_native_provider_operations(state, &app_type);

        let mut provider = provider;
        Self::prepare_new_provider(&app_type, &mut provider)?;
        if app_type.is_additive_mode() {
            Self::set_provider_live_config_managed(&mut provider, add_to_live);
        }

        if matches!(app_type, AppType::Codex) {
            return Self::add_codex(state, provider);
        }

        if matches!(app_type, AppType::Dsh) {
            // The key is user-chosen, and saving would overwrite an existing row or live node.
            // The per-app lock also serializes native list refreshes and other mutations;
            // the live write repeats the check under dsh's file lock.
            if state
                .db
                .get_provider_by_id(&provider.id, &app_type)?
                .is_some()
                || crate::dsh_config::provider_key_exists(&provider.id)?
            {
                return Err(AppError::InvalidInput(format!(
                    "{} provider key '{}' already exists",
                    app_type.as_str(),
                    provider.id
                )));
            }
            return Self::save_native_provider(state, &app_type, &provider, add_to_live, true);
        }

        // 还没有当前供应商时这一家会被写进 live：和进入代理、切换互斥（同 `update`）。
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;

        // 累加式写 live 失败（如配置文件解析不了）时要撤回这一行，先记下原来的
        EditorSaveKind::Add.validate_target(state, &app_type, &provider.id)?;
        let existing_row = if app_type.is_additive_mode() {
            state.db.get_provider_by_id(&provider.id, &app_type)?
        } else {
            None
        };

        // Save to database
        state.db.save_provider(&app_type, &provider)?;

        // Additive mode apps: optionally write to live config.
        if app_type.is_additive_mode() {
            // OMO / OMO Slim providers use exclusive mode and write to dedicated config file.
            if matches!(app_type, AppType::OpenCode)
                && matches!(provider.category.as_deref(), Some("omo") | Some("omo-slim"))
            {
                // Do not auto-enable newly added OMO / OMO Slim providers.
                // Users must explicitly switch/apply an OMO provider to activate it.
                return Ok(true);
            }
            if !add_to_live {
                return Ok(true);
            }
            return Self::keep_row_if_written(
                state,
                &app_type,
                &provider.id,
                existing_row.as_ref(),
                write_live_for_state(state, &app_type, &provider),
            );
        }

        // For other apps: Check if sync is needed (if this is current provider, or no current provider)
        let current = crate::mode::current::provider_for(
            &state.db,
            &app_type,
            crate::mode::current::Purpose::Direct,
        )?;
        if current.is_none() {
            // 第一个供应商同样只写关键字段，不覆盖用户已有的配置文件。
            match app_type {
                AppType::Claude => {
                    return Self::keep_row_if_written(
                        state,
                        &app_type,
                        &provider.id,
                        None,
                        claude_direct::switch_to(state.db.as_ref(), None, &provider).map(|_| ()),
                    );
                }

                // Antigravity 的卡只用来打开终端，添加不写任何文件。
                AppType::Agy => return Ok(true),

                _ => {}
            }
            // No current provider, set as current and sync. Managed Codex adds
            // use the transactional path above because token resolution can fail.
            state.db.set_current_provider(&app_type, &provider.id)?;
            write_live_for_state(state, &app_type, &provider)?;
        }

        Ok(true)
    }

    /// 新增 Codex 供应商。还没有当前供应商时，它就是第一个：行、live 和指针一起提交，
    /// live 只写关键字段，不覆盖用户已有的 config.toml。写 live 失败时撤回刚存的行，
    /// 免得留下一个看得见却用不了的供应商。
    fn add_codex(state: &AppState, provider: Provider) -> Result<bool, AppError> {
        let app_type = AppType::Codex;
        // 和切换互斥：等着的切换不能看到只存了一半的托管账号绑定。
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;
        EditorSaveKind::Add.validate_target(state, &app_type, &provider.id)?;
        let current = crate::mode::current::provider_for(
            &state.db,
            &app_type,
            crate::mode::current::Purpose::Direct,
        )?;
        if current.is_some() {
            state.db.save_provider(&app_type, &provider)?;
            return Ok(true);
        }

        // 行有问题（会把官方登录发给第三方等）在存行之前就拒绝。
        codex_direct::preflight(state.db.as_ref(), &provider)?;
        let previous = state.db.get_provider_by_id(&provider.id, &app_type)?;
        state.db.save_provider(&app_type, &provider)?;
        let written = codex_direct::write_direct(
            state.db.as_ref(),
            &state.codex_oauth_manager,
            crate::mode::state::op::SWITCH,
            codex_direct::Owner::None,
            Some(&provider),
            crate::mode::state::PendingTarget::pointer(Some(provider.id.clone())),
        );
        if let Err(error) = written {
            // 文件已经发布、只是落定状态失败时，pending 会在下次操作或启动时补完指针，
            // 行要留着；还没发布就失败，撤回刚存的行。
            if crate::mode::operation::has_pending(AppType::Codex.as_str()) {
                return Err(error);
            }
            let rollback = match &previous {
                Some(previous) => state.db.save_provider(&app_type, previous),
                None => state.db.delete_provider(&app_type, &provider.id),
            };
            if let Err(rollback) = rollback {
                return Err(AppError::Message(format!(
                    "新增首个 Codex 供应商失败: {error}; 恢复供应商数据同时失败: {rollback}"
                )));
            }
            return Err(error);
        }
        Ok(true)
    }

    /// 供应商编辑器底部配置的显示内容（Claude Code、Codex）：切到
    /// 这个供应商之后配置文件会是什么样，以及行里不随切换生效的字段。`category` 用来认出
    /// 官方卡。
    pub fn editor_view(
        state: &AppState,
        app_type: AppType,
        settings_config: &Value,
        category: Option<&str>,
    ) -> Result<EditorView, AppError> {
        Self::editor_view_with_meta(state, app_type, settings_config, category, None, None)
    }

    /// Codex previews retain managed-provider identity. Explicit draft metadata,
    /// including an empty value, takes precedence over the stored card's metadata.
    pub fn editor_view_with_meta(
        state: &AppState,
        app_type: AppType,
        settings_config: &Value,
        category: Option<&str>,
        provider_id: Option<&str>,
        meta: Option<&ProviderMeta>,
    ) -> Result<EditorView, AppError> {
        match app_type {
            AppType::Claude => claude_editor::view(state, settings_config),
            AppType::Codex => {
                let stored_meta = match (meta, provider_id) {
                    (None, Some(id)) => state
                        .db
                        .get_provider_by_id(id, &AppType::Codex)?
                        .and_then(|provider| provider.meta),
                    _ => None,
                };
                codex_editor::view(
                    state,
                    settings_config,
                    category,
                    meta.or(stored_meta.as_ref()),
                )
            }

            other => Err(AppError::InvalidInput(format!(
                "{} 的编辑器还不支持按关键字段显示",
                other.as_str()
            ))),
        }
    }

    /// 编辑已有供应商时给 [`Self::editor_view`] 的 `category`：按库里那一行，用切换时同一个
    /// 判断认官方卡。Gemini 还按合作方标记和名字认出旧版没有 `category` 的 Google 卡，Codex
    /// 还认固定 id 和托管账号；只看 `category` 的话，这些卡预览里的登录方式和切换写的不同。
    /// 没有 `provider_id`（新增）或行不存在时原样用 `category`。
    pub fn editor_category(
        state: &AppState,
        app_type: &AppType,
        provider_id: Option<&str>,
        category: Option<String>,
    ) -> Result<Option<String>, AppError> {
        let Some(id) = provider_id else {
            return Ok(category);
        };
        let Some(row) = state.db.get_provider_by_id(id, app_type)? else {
            return Ok(category);
        };
        let official = match app_type {
            AppType::Codex => codex_direct::is_official(&row),

            AppType::Agy => agy_direct::is_official(&row),
            _ => false,
        };
        Ok(if official {
            Some("official".to_string())
        } else {
            category
        })
    }

    /// 从编辑器新增供应商。Claude Code、Codex 按关键字段拆开保存
    /// （见各自的 `*_editor`），其余应用和 `add` 一样。
    pub fn add_from_editor(
        state: &AppState,
        app_type: AppType,
        provider: Provider,
        add_to_live: bool,
        editor: Option<EditorSave>,
    ) -> Result<bool, AppError> {
        match (app_type, editor) {
            (AppType::Claude, Some(editor)) => {
                Self::add_claude_from_editor(state, provider, editor)
            }
            (AppType::Codex, Some(editor)) => {
                Self::save_codex_from_editor(state, provider, editor, EditorSaveKind::Add)
            }

            (app_type, _) => Self::add(state, app_type, provider, add_to_live),
        }
    }

    /// 从编辑器保存供应商。Claude Code、Codex 按关键字段拆开保存
    /// （见各自的 `*_editor`），其余应用和 `update` 一样。
    pub fn update_from_editor(
        state: &AppState,
        app_type: AppType,
        original_id: Option<&str>,
        provider: Provider,
        editor: Option<EditorSave>,
    ) -> Result<bool, AppError> {
        match (app_type, editor) {
            (AppType::Claude, Some(editor))
                if original_id.is_none_or(|original| original == provider.id) =>
            {
                Self::update_claude_from_editor(state, provider, editor)
            }
            (AppType::Codex, Some(editor))
                if original_id.is_none_or(|original| original == provider.id) =>
            {
                Self::save_codex_from_editor(state, provider, editor, EditorSaveKind::Update)
            }

            (app_type, _) => Self::update(state, app_type, original_id, provider),
        }
    }

    /// 从编辑器新增或保存 Codex 供应商：关键字段、独有字段存回行，其余改动作为全局设置
    /// 写进 live（三方比较）。直连模式下编辑当前供应商、或新增第一个供应商时，关键字段在
    /// 同一次写入里换进 live；代理模式下编辑路由那家，全局设置写完后按新行重写代理契约。
    fn save_codex_from_editor(
        state: &AppState,
        provider: Provider,
        editor: EditorSave,
        kind: EditorSaveKind,
    ) -> Result<bool, AppError> {
        let app_type = AppType::Codex;
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;
        kind.validate_target(state, &app_type, &provider.id)?;
        let existing = state.db.get_provider_by_id(&provider.id, &app_type)?;
        let mut provider = provider;
        let plan = codex_editor::plan_save(
            existing.as_ref().map(|row| &row.settings_config),
            &provider.settings_config,
            &editor.base,
            &match (existing.as_ref(), editor.draft.as_ref()) {
                (Some(row), _) => codex_editor::Origin::row(&row.settings_config)?,
                (None, Some(draft)) => codex_editor::Origin::row(draft)?,
                (None, None) => codex_editor::Origin::Live(codex_editor::live_exclusive(state)?),
            },
            codex_direct::is_official(&provider),
            editor.on_conflict,
        )?;
        provider.settings_config = plan.row_settings.clone();
        Self::validate_provider_settings(&app_type, &provider)?;
        Self::normalize_usage_script_credential_overrides(&app_type, &mut provider);
        if kind == EditorSaveKind::Add {
            keep_common_config_for_old_versions(&mut provider);
        }

        let mode = crate::mode::current::mode_state(&app_type);
        let key_fields = kind.writes_key_fields(state, &app_type, &mode, &provider.id)?;

        state.db.save_provider(&app_type, &provider)?;
        let written = if key_fields {
            codex_editor::write_live(
                state.db.as_ref(),
                &state.codex_oauth_manager,
                &plan.edits,
                codex_editor::KeyFields::Direct {
                    prev: existing.as_ref(),
                    target: &provider,
                    set_pointer: kind == EditorSaveKind::Add,
                },
            )
        } else {
            codex_editor::write_live(
                state.db.as_ref(),
                &state.codex_oauth_manager,
                &plan.edits,
                codex_editor::KeyFields::None,
            )
            .and_then(|()| Self::resync_proxy_for_saved_row(state, &app_type, &provider))
        };
        Self::keep_row_if_written(state, &app_type, &provider.id, existing.as_ref(), written)
    }

    /// 编辑器保存先存行、再写 live（`written`）。写 live 失败时：文件已经发布、只是落定
    /// 状态失败的，pending 会在下次操作或启动时补完，行要留着；还没发布就失败（没有
    /// pending），撤回刚存的行。
    fn keep_row_if_written(
        state: &AppState,
        app_type: &AppType,
        provider_id: &str,
        existing: Option<&Provider>,
        written: Result<(), AppError>,
    ) -> Result<bool, AppError> {
        let Err(error) = written else {
            return Ok(true);
        };
        if crate::mode::operation::has_pending(app_type.as_str()) {
            return Err(error);
        }
        let rollback = match existing {
            Some(existing) => state.db.save_provider(app_type, existing),
            None => state.db.delete_provider(app_type, provider_id),
        };
        if let Err(rollback) = rollback {
            log::error!(
                "恢复 {} 供应商 '{provider_id}' 失败: {rollback}",
                app_type.as_str()
            );
            return Err(AppError::Message(format!(
                "{error}; DB rollback also failed: {rollback}"
            )));
        }
        Err(error)
    }

    /// 代理模式下存好了这一行：它是代理路由或在 Stack 名单里时按新行重写代理契约，契约没变
    /// 就不碰客户端文件（见 [`crate::mode::controller::resync_saved_row_locked`]）。调用方
    /// 持有这个应用的切换锁。
    fn resync_proxy_for_saved_row(
        state: &AppState,
        app_type: &AppType,
        provider: &Provider,
    ) -> Result<(), AppError> {
        futures::executor::block_on(crate::mode::controller::resync_saved_row_locked(
            state, app_type, provider,
        ))
        .map_err(AppError::Message)
    }

    fn add_claude_from_editor(
        state: &AppState,
        provider: Provider,
        editor: EditorSave,
    ) -> Result<bool, AppError> {
        let app_type = AppType::Claude;
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;
        EditorSaveKind::Add.validate_target(state, &app_type, &provider.id)?;
        let mut provider = provider;
        Self::normalize_provider_if_claude(&app_type, &mut provider);
        let plan = claude_editor::plan_save(None, &provider.settings_config, &editor.base)?;
        provider.settings_config = plan.row_settings.clone();
        Self::validate_provider_settings(&app_type, &provider)?;
        Self::normalize_usage_script_credential_overrides(&app_type, &mut provider);
        keep_common_config_for_old_versions(&mut provider);

        let existing = state.db.get_provider_by_id(&provider.id, &app_type)?;
        let first = crate::mode::current::provider_for(
            &state.db,
            &app_type,
            crate::mode::current::Purpose::Direct,
        )?
        .is_none();
        state.db.save_provider(&app_type, &provider)?;

        let key_fields = first.then_some(claude_editor::KeyFieldWrite {
            prev: None,
            target: &provider,
            set_pointer: true,
        });
        let written =
            claude_editor::write_live(state.db.as_ref(), &plan, editor.on_conflict, key_fields);
        Self::keep_row_if_written(state, &app_type, &provider.id, existing.as_ref(), written)
    }

    /// 和其他编辑器一样先存行、再写 live：写 live 失败且没有 pending 时撤回行；已经发布的
    /// 由 pending 补完，行和 live 不会对不上。
    fn update_claude_from_editor(
        state: &AppState,
        provider: Provider,
        editor: EditorSave,
    ) -> Result<bool, AppError> {
        let app_type = AppType::Claude;
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;
        EditorSaveKind::Update.validate_target(state, &app_type, &provider.id)?;
        let existing = state.db.get_provider_by_id(&provider.id, &app_type)?;
        let mut provider = provider;
        Self::normalize_provider_if_claude(&app_type, &mut provider);
        let plan = claude_editor::plan_save(
            existing.as_ref().map(|row| &row.settings_config),
            &provider.settings_config,
            &editor.base,
        )?;
        provider.settings_config = plan.row_settings.clone();
        Self::validate_provider_settings(&app_type, &provider)?;
        Self::normalize_usage_script_credential_overrides(&app_type, &mut provider);

        let mode = crate::mode::current::mode_state(&app_type);
        // 代理模式下 live 的关键字段是代理契约，这里只写全局改动；编辑的是代理路由那家
        // 或 Stack 名单里的一家时，写完按新行重写契约（契约没变就不动）。直连指针那家在
        // 退出代理时写回。
        let key_fields = EditorSaveKind::Update
            .writes_key_fields(state, &app_type, &mode, &provider.id)?
            .then_some(claude_editor::KeyFieldWrite {
                prev: existing.as_ref(),
                target: &provider,
                set_pointer: false,
            });

        state.db.save_provider(&app_type, &provider)?;
        let written =
            claude_editor::write_live(state.db.as_ref(), &plan, editor.on_conflict, key_fields)
                .and_then(|()| Self::resync_proxy_for_saved_row(state, &app_type, &provider));
        Self::keep_row_if_written(state, &app_type, &provider.id, existing.as_ref(), written)
    }

    /// Update a provider
    pub fn update(
        state: &AppState,
        app_type: AppType,
        original_id: Option<&str>,
        provider: Provider,
    ) -> Result<bool, AppError> {
        let _native_guard = lock_native_provider_operations(state, &app_type);
        let mut provider = provider;

        let original_id = original_id.unwrap_or(provider.id.as_str()).to_string();
        let provider_id_changed = original_id != provider.id;
        // 读旧行、判断谁是当前供应商、存行、写 live 都在切换锁里：进入代理和切换都等这次
        // 保存写完，这里也不会在读完模式之后、写 live 之前被它们改掉模式（否则直连的关键
        // 字段会盖掉刚写的代理契约）。Codex 还要在锁里读旧行，看它是不是托管账号的行。
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;
        let existing_provider = state.db.get_provider_by_id(&original_id, &app_type)?;
        if !provider_id_changed {
            EditorSaveKind::Update.validate_target(state, &app_type, &original_id)?;
        }
        // Normalize Claude model keys
        Self::normalize_provider_if_claude(&app_type, &mut provider);
        Self::validate_provider_settings(&app_type, &provider)?;
        if matches!(app_type, AppType::Codex) && provider.has_official_category() {
            crate::codex_config::strip_codex_unified_session_bucket_from_settings(
                &mut provider.settings_config,
            )?;
        }
        Self::normalize_usage_script_credential_overrides(&app_type, &mut provider);

        if provider_id_changed {
            if !app_type.is_additive_mode() {
                return Err(AppError::Message(
                    "Only additive-mode providers support changing provider key".to_string(),
                ));
            }

            let Some(existing_provider) = existing_provider else {
                return Err(AppError::Message(format!(
                    "Original provider '{}' does not exist in app '{}'",
                    original_id,
                    app_type.as_str()
                )));
            };

            // OMO / OMO Slim providers are activated via a dedicated current-state mechanism
            // (set_omo_provider_current) that is NOT captured by provider_exists_in_live_config,
            // which only checks opencode.json. A rename would orphan that current-state marker
            // and silently break subsequent OMO file syncs. Block it unconditionally.
            if matches!(app_type, AppType::OpenCode)
                && matches!(
                    existing_provider.category.as_deref(),
                    Some("omo") | Some("omo-slim")
                )
            {
                return Err(AppError::Message(
                    "Provider key cannot be changed for OMO/OMO Slim providers".to_string(),
                ));
            }

            let original_in_live = Self::check_live_config_exists(
                &app_type,
                &original_id,
                Self::provider_live_config_managed(&existing_provider),
            )?;
            if original_in_live {
                return Err(AppError::Message(
                    "Provider key cannot be changed after the provider has been added to the app config"
                        .to_string(),
                ));
            }

            let next_id_in_live = Self::check_live_config_exists(
                &app_type,
                &provider.id,
                Self::provider_live_config_managed(&existing_provider),
            )?;
            if state
                .db
                .get_provider_by_id(&provider.id, &app_type)?
                .is_some()
                || next_id_in_live
            {
                return Err(AppError::Message(format!(
                    "Provider '{}' already exists in app '{}'",
                    provider.id,
                    app_type.as_str()
                )));
            }

            Self::set_provider_live_config_managed(&mut provider, false);
            let was_current = crate::mode::current::local_direct_pointer(&app_type).as_deref()
                == Some(original_id.as_str());
            state
                .db
                .replace_provider_id(&app_type, &original_id, &provider)?;

            if was_current {
                crate::settings::set_current_provider(&app_type, Some(provider.id.as_str()))?;
            }

            return Ok(true);
        }

        // Additive mode apps: only sync to live when the provider
        // already exists in live config. Editing a DB-only provider must not auto-add it.
        if app_type.is_additive_mode() {
            let omo_variant = if matches!(app_type, AppType::OpenCode) {
                match provider.category.as_deref() {
                    Some("omo") => Some(&crate::services::omo::STANDARD),
                    Some("omo-slim") => Some(&crate::services::omo::SLIM),
                    _ => None,
                }
            } else {
                None
            };
            if let Some(variant) = omo_variant {
                let is_current =
                    state
                        .db
                        .is_omo_provider_current(&app_type, &provider.id, variant.category)?;
                if is_current {
                    crate::services::OmoService::write_provider_config_to_file(&provider, variant)?;
                }
                if let Err(err) = state.db.save_provider(&app_type, &provider) {
                    if is_current {
                        if let Err(rollback_err) =
                            crate::services::OmoService::write_config_to_file(state, variant)
                        {
                            log::warn!(
                                "Failed to roll back {} config after DB save error: {}",
                                variant.label,
                                rollback_err
                            );
                        }
                    }
                    return Err(err);
                }
                return Ok(true);
            }
            let live_config_managed = Self::check_live_config_exists(
                &app_type,
                &provider.id,
                Self::provider_live_config_managed(&provider).or_else(|| {
                    existing_provider
                        .as_ref()
                        .and_then(Self::provider_live_config_managed)
                }),
            )?;
            Self::set_provider_live_config_managed(&mut provider, live_config_managed);

            if matches!(app_type, AppType::Dsh) {
                return Self::save_native_provider(
                    state,
                    &app_type,
                    &provider,
                    live_config_managed,
                    false,
                );
            }

            // Save to database after live-config presence is resolved so parse errors
            // do not report failure after already mutating DB state. Live 写失败时
            // 回滚数据库，避免「DB 新值、客户端旧值」的部分提交（与 Mcode 路径一致）。
            let previous = existing_provider.clone();
            state.db.save_provider(&app_type, &provider)?;

            if !live_config_managed {
                return Ok(true);
            }
            if let Err(error) = write_live_for_state(state, &app_type, &provider) {
                match previous {
                    Some(previous) => {
                        if let Err(rollback_err) = state.db.save_provider(&app_type, &previous) {
                            log::warn!(
                                "Failed to roll back additive provider {} after live write error: {}",
                                provider.id,
                                rollback_err
                            );
                        }
                    }
                    None => {
                        if let Err(rollback_err) = state.db.delete_provider(&app_type, &provider.id)
                        {
                            log::warn!(
                                "Failed to roll back new additive provider {} after live write error: {}",
                                provider.id,
                                rollback_err
                            );
                        }
                    }
                }
                return Err(error);
            }
            return Ok(true);
        }

        // For other apps: 是否是直连指针那家。代理模式下是不是代理路由或 Stack 名单里的一家，
        // 由同步 live 的那一步判断。
        let mode = crate::mode::current::mode_state(&app_type);
        let is_direct_current = crate::mode::current::provider_for(
            &state.db,
            &app_type,
            crate::mode::current::Purpose::Direct,
        )?
        .as_deref()
            == Some(provider.id.as_str());

        if matches!(app_type, AppType::Codex) {
            return Self::update_codex(
                state,
                &provider,
                existing_provider.as_ref(),
                is_direct_current,
            );
        }
        // Desktop 直连模式下正在用的这张卡改成了要经路由的（模型映射、托管账号）：直连写不
        // 进去，存了行也同步不到 Desktop。先进入路由模式再改。

        // Save to database
        state.db.save_provider(&app_type, &provider)?;

        if is_direct_current || mode.is_proxy() {
            let outcome = live::sync_live_for_provider_respecting_mode(
                state,
                &app_type,
                &provider,
                existing_provider.as_ref(),
            )?;
            if outcome == LiveSyncOutcome::WroteLive {
                // MCP is stored in the database and projected after a successful
                // live write. Keep the failure best-effort so the provider save
                // itself is not reported as failed when MCP projection can retry.
                if let Err(err) = McpService::sync_enabled_for_app(state, &app_type) {
                    log::warn!(
                        "保存供应商后重投影 {app_type:?} MCP 失败（将在下次同步时自愈）: {err}"
                    );
                }
            }
        }

        Ok(true)
    }

    /// 保存 Codex 供应商。调用方持有这个应用的切换锁。
    ///
    /// - 直连模式下编辑直连那家：先存行，再只替换 live 里的关键字段和独有字段（换托管
    ///   账号时先采纳、再清掉旧账号的登录）；写 live 失败就把行恢复原样。
    /// - 代理模式下先存行，编辑的是路由那家或 Stack 名单里的一家时按新行重写代理契约（见
    ///   [`Self::resync_proxy_for_saved_row`]），写失败同样把行恢复原样。
    /// - 其余只存行。
    fn update_codex(
        state: &AppState,
        provider: &Provider,
        existing: Option<&Provider>,
        is_direct_current: bool,
    ) -> Result<bool, AppError> {
        let app_type = AppType::Codex;
        let mode = crate::mode::current::mode_state(&app_type);
        if !mode.is_proxy() && !is_direct_current {
            state.db.save_provider(&app_type, provider)?;
            return Ok(true);
        }
        if !mode.is_proxy() {
            codex_direct::preflight(state.db.as_ref(), provider)?;
        }

        state.db.save_provider(&app_type, provider)?;
        let written = if mode.is_proxy() {
            Self::resync_proxy_for_saved_row(state, &app_type, provider)
        } else {
            codex_direct::write_direct(
                state.db.as_ref(),
                &state.codex_oauth_manager,
                crate::mode::state::op::APPLY,
                existing.map_or(codex_direct::Owner::None, codex_direct::Owner::Provider),
                Some(provider),
                crate::mode::state::PendingTarget::default(),
            )
            .map(|_| ())
        };
        if let Err(error) = written {
            if crate::mode::operation::has_pending(AppType::Codex.as_str()) {
                return Err(error);
            }
            if let Some(existing) = existing {
                if let Err(rollback) = state.db.save_provider(&app_type, existing) {
                    return Err(AppError::Message(format!(
                        "更新 Codex 供应商失败: {error}; 恢复供应商数据同时失败: {rollback}"
                    )));
                }
            }
            return Err(error);
        }
        Ok(true)
    }

    /// Delete a provider
    ///
    /// 直连指针或代理路由指着的供应商不能删（见 `mode::current::is_referenced`）。
    /// 对于累加模式应用（OpenCode, OpenClaw），可以随时删除任意供应商，同时从 live 配置中移除。
    pub fn delete(state: &AppState, app_type: AppType, id: &str) -> Result<(), AppError> {
        let _native_guard = lock_native_provider_operations(state, &app_type);
        if state.db.get_unreadable_provider(&app_type, id)?.is_some() {
            return Self::delete_unreadable(state, &app_type, id);
        }

        if matches!(app_type, AppType::Dsh) {
            return crate::dsh_config::save_and_commit(id, None, false, || {
                state.db.delete_provider(&app_type, id)
            });
        }

        // 与切换共用应用锁：累加式删 live / 删 DB 也不能被 switch 插入旧快照写 live。
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;

        // Additive mode apps - no current provider concept
        if app_type.is_additive_mode() {
            // Single DB read shared across all additive-mode sub-paths below.
            let existing = state.db.get_provider_by_id(id, &app_type)?;

            if matches!(app_type, AppType::OpenCode) {
                let provider_category = existing.as_ref().and_then(|p| p.category.clone());
                let omo_variant = match provider_category.as_deref() {
                    Some("omo") => Some(&crate::services::omo::STANDARD),
                    Some("omo-slim") => Some(&crate::services::omo::SLIM),
                    _ => None,
                };
                if let Some(variant) = omo_variant {
                    let was_current =
                        state
                            .db
                            .is_omo_provider_current(&app_type, id, variant.category)?;
                    if was_current {
                        // 先删配置文件，DB 作为 commit：文件侧失败不删卡；DB 失败会回滚文件
                        crate::services::OmoService::delete_config_file_with_commit(
                            variant,
                            || state.db.delete_provider(&app_type, id),
                        )?;
                    } else {
                        state.db.delete_provider(&app_type, id)?;
                    }
                    return Ok(());
                }
            }

            // Non-OMO：先剥 live 再删 DB。DB 失败时尝试把 live 写回，避免「列表还在、live 已空」。
            //
            // Use check_live_config_exists rather than trusting the flag alone: the flag
            // can be stale (Some(false) for a provider that was written to live before the
            // live_config_managed flip was introduced). check_live_config_exists reads the
            // actual file when the flag is Some(false), so it handles historical data correctly.
            let live_managed = existing
                .as_ref()
                .and_then(Self::provider_live_config_managed);
            let removed_from_live = if Self::check_live_config_exists(&app_type, id, live_managed)?
            {
                match app_type {
                    AppType::OpenCode => remove_opencode_provider_from_live(id)?,

                    AppType::Dsh => crate::dsh_config::remove_provider(id)?,
                    _ => {}
                }
                true
            } else {
                false
            };
            if let Err(error) = state.db.delete_provider(&app_type, id) {
                if removed_from_live {
                    if let Some(provider) = existing.as_ref() {
                        if let Err(restore) = write_live_for_state(state, &app_type, provider) {
                            log::error!(
                                "删除 {} 供应商 '{id}' 的 DB 行失败，且 live 回写也失败: {error}; restore={restore}",
                                app_type.as_str()
                            );
                            return Err(AppError::Message(format!(
                                "{error}; live restore also failed: {restore}"
                            )));
                        }
                    }
                }
                return Err(error);
            }
            return Ok(());
        }

        // For other apps: 本地记录、DB、代理路由任何一处指着它都不能删。Antigravity 没有
        // 「启用」，旧版留下的直连指针不拦删除。
        if app_type != AppType::Agy
            && crate::mode::current::is_referenced(&state.db, &app_type, id)?
        {
            return Err(AppError::Message(
                "无法删除当前正在使用的供应商".to_string(),
            ));
        }

        // Stack 名单里的先移出（和客户端文件同一个操作提交，key 留在登记簿里），成功了再删行。
        // 删行失败时它已经不在名单里，重新加入即可。
        if crate::mode::stack::is_member(&app_type, id)? {
            crate::block_on(crate::mode::controller::set_stack_member_locked(
                state, &app_type, id, false,
            ))
            .map_err(AppError::Message)?;
        }

        state.db.delete_provider(&app_type, id)?;
        Ok(())
    }

    /// 读不出来的行只删库里这一行：不知道它往 live 写过什么，所以不碰 live，也不写回任何东西。
    /// 正被使用的照样不能删。
    fn delete_unreadable(state: &AppState, app_type: &AppType, id: &str) -> Result<(), AppError> {
        if !app_type.is_additive_mode()
            && *app_type != AppType::Agy
            && crate::mode::current::is_referenced(&state.db, app_type, id)?
        {
            return Err(AppError::Message(
                "无法删除当前正在使用的供应商".to_string(),
            ));
        }
        state.db.delete_provider(app_type, id)
    }

    /// Remove provider from live config only (for additive mode apps like OpenCode, OpenClaw)
    ///
    /// Does NOT delete from database - provider remains in the list.
    /// This is used when user wants to "remove" a provider from active config
    /// but keep it available for future use.
    pub fn remove_from_live_config(
        state: &AppState,
        app_type: AppType,
        id: &str,
    ) -> Result<(), AppError> {
        let _native_guard = lock_native_provider_operations(state, &app_type);
        // OpenCode/OpenClaw/Hermes 剥 live 与 switch 写 live 必须串行
        let _switch_guard = matches!(app_type, AppType::OpenCode)
            .then(|| crate::mode::controller::lock_settled_blocking(state, &app_type))
            .transpose()?;

        if matches!(app_type, AppType::Dsh) {
            return crate::dsh_config::save_and_commit(id, None, false, || {
                if let Some(mut provider) = state.db.get_provider_by_id(id, &app_type)? {
                    Self::set_provider_live_config_managed(&mut provider, false);
                    state.db.save_provider(&app_type, &provider)?;
                }
                Ok(())
            });
        }

        match app_type {
            AppType::OpenCode => {
                let provider_category = state
                    .db
                    .get_provider_by_id(id, &app_type)?
                    .and_then(|p| p.category);

                let omo_variant = match provider_category.as_deref() {
                    Some("omo") => Some(&crate::services::omo::STANDARD),
                    Some("omo-slim") => Some(&crate::services::omo::SLIM),
                    _ => None,
                };
                if let Some(variant) = omo_variant {
                    if state
                        .db
                        .is_omo_provider_current(&app_type, id, variant.category)?
                    {
                        crate::services::OmoService::delete_config_file_with_commit(
                            variant,
                            || {
                                state
                                    .db
                                    .clear_omo_provider_current(&app_type, id, variant.category)
                            },
                        )?;
                    }
                } else {
                    remove_opencode_provider_from_live(id)?;
                }
            }

            AppType::Dsh => crate::dsh_config::remove_provider(id)?,
            _ => {
                return Err(AppError::Message(format!(
                    "App {} does not support remove from live config",
                    app_type.as_str()
                )));
            }
        }

        if let Some(mut provider) = state.db.get_provider_by_id(id, &app_type)? {
            Self::set_provider_live_config_managed(&mut provider, false);
            state.db.save_provider(&app_type, &provider)?;
        }

        Ok(())
    }

    /// 切换供应商。
    ///
    /// - 代理模式：只换代理路由，直连指针不变；契约没变时客户端文件不读也不写。
    /// - 直连模式：切换式应用只替换客户端文件里的关键字段（不回填），文件和指针在同一个
    ///   操作里提交；累加式应用按各自的规则写入。
    pub fn switch(state: &AppState, app_type: AppType, id: &str) -> Result<SwitchResult, AppError> {
        let app = app_type.as_str().to_string();
        let result = Self::switch_inner(state, app_type, id);
        match &result {
            Ok(result) if result.warnings.is_empty() => {
                log::info!("[SWITCH] {app} 切到 {id} 完成");
            }
            Ok(result) => {
                log::warn!(
                    "[SWITCH] {app} 切到 {id} 完成，有警告: {:?}",
                    result.warnings
                );
            }
            // 规则拒绝：什么都没改。
            Err(error @ AppError::Localized { key, .. })
                if *key == "switch.official_blocked_by_proxy"
                    || *key == "agy.switch.unsupported" =>
            {
                log::info!("[SWITCH] {app} 拒绝切到 {id}: {error}");
            }
            Err(error) => log::error!(
                "[SWITCH] {app} 切到 {id} 失败: {}",
                crate::error_for_log(&error.to_string())
            ),
        }
        result
    }

    fn switch_inner(
        state: &AppState,
        app_type: AppType,
        id: &str,
    ) -> Result<SwitchResult, AppError> {
        let _native_guard = lock_native_provider_operations(state, &app_type);

        // Antigravity 的卡只作用于它打开的终端，没有「启用」（见 agy_config 模块说明）。
        if app_type == AppType::Agy {
            return Err(AppError::localized(
                "agy.switch.unsupported",
                "Antigravity 的卡片不需要启用，用「打开终端」启动即可",
                "Antigravity cards are not enabled; use Open terminal to start one",
            ));
        }

        // Check if provider exists. 别的行读不出来不拦这次切换（它们在下面只用来找上一家；
        // 上一家读不出来就和它不在库里一样处理）；要切到的这一行读不出来则报错。
        let rows = state.db.get_provider_rows(&app_type)?;
        let _provider = rows
            .get(id)?
            .ok_or_else(|| AppError::Message(format!("供应商 {id} 不存在")))?;
        let providers = &rows.readable;

        if matches!(app_type, AppType::Dsh) {
            let mut provider = _provider.clone();
            Self::set_provider_live_config_managed(&mut provider, true);
            crate::dsh_config::save_and_commit(
                id,
                Some(provider.settings_config.clone()),
                false,
                || state.db.save_provider(&app_type, &provider),
            )?;
            return Ok(SwitchResult::default());
        }

        // OMO providers are switched through their own exclusive path.
        if matches!(app_type, AppType::OpenCode) && _provider.category.as_deref() == Some("omo") {
            return Self::switch_normal(state, app_type, id, providers);
        }

        // OMO Slim providers are switched through their own exclusive path.
        if matches!(app_type, AppType::OpenCode)
            && _provider.category.as_deref() == Some("omo-slim")
        {
            return Self::switch_normal(state, app_type, id, providers);
        }

        // 切换和进入 / 退出代理都会改客户端文件和指针。按应用串行，拿到锁、补完上一次
        // 没做完的写入之后再读模式和指针：刚进入代理的应用不会被一次直连写入覆盖，上次
        // 失败后重试也按补完后的指针删上一家的独有字段。
        let _switch_guard = crate::mode::controller::lock_settled_blocking(state, &app_type)?;

        // 锁外预读的快照可能已被并发 update 改掉；锁内重读再写 live，避免旧密钥覆盖新配置。
        let rows = state.db.get_provider_rows(&app_type)?;
        let _provider = rows
            .get(id)?
            .ok_or_else(|| AppError::Message(format!("供应商 {id} 不存在")))?;
        let providers = &rows.readable;

        if crate::mode::current::is_proxy(&app_type) {
            // 代理模式：只换代理路由，直连指针不变。契约没变时客户端文件不读也不写；
            // 变了在同一个操作里先改写客户端，再发布路由。
            if _provider.has_official_category()
                && !official_provider_supports_proxy_takeover(&app_type, _provider)
            {
                return Err(AppError::localized(
                    "switch.official_blocked_by_proxy",
                    "路由模式下不能切换到官方供应商，使用代理访问官方 API 可能导致账号被封禁。请先退出路由模式，或选择第三方供应商。",
                    "Cannot switch to an official provider in routing mode. Using a proxy with official APIs may cause account bans.",
                ));
            }
            log::info!(
                "[SWITCH] {} 代理模式：路由 {} → {}",
                app_type.as_str(),
                crate::mode::current::mode_state(&app_type)
                    .proxy_route
                    .as_deref()
                    .unwrap_or("（无）"),
                id
            );
            let route = futures::executor::block_on(crate::mode::controller::switch_route_locked(
                state, &app_type, _provider,
            ))
            .map_err(|e| AppError::Message(format!("切换路由失败: {e}")))?;
            // MCP 不随路由变：客户端文件没按直连重写。
            return Ok(SwitchResult {
                route: Some(route),
                ..Default::default()
            });
        }

        // Normal mode: full switch with Live config write
        Self::switch_normal(state, app_type, id, providers)
    }

    /// Normal switch flow (non-proxy mode)
    fn switch_normal(
        state: &AppState,
        app_type: AppType,
        id: &str,
        providers: &indexmap::IndexMap<String, Provider>,
    ) -> Result<SwitchResult, AppError> {
        let provider = providers
            .get(id)
            .ok_or_else(|| AppError::Message(format!("供应商 {id} 不存在")))?;

        if app_type.is_additive_mode() {
            log::info!("[SWITCH] {} 写入 {id}（累加式应用）", app_type.as_str());
        } else {
            let previous = crate::mode::current::provider_for(
                &state.db,
                &app_type,
                crate::mode::current::Purpose::Direct,
            )
            .ok()
            .flatten();
            log::info!(
                "[SWITCH] {} 直连：{} → {id}，写客户端配置",
                app_type.as_str(),
                previous.as_deref().unwrap_or("（无）")
            );
        }

        if matches!(app_type, AppType::Claude) {
            return Self::switch_claude_direct(state, provider, providers);
        }
        if matches!(app_type, AppType::Codex) {
            return Self::switch_codex_direct(state, provider, providers);
        }

        // OMO ↔ OMO Slim are mutually exclusive; activating one removes the other's config file.
        if matches!(app_type, AppType::OpenCode) {
            let omo_pair = match provider.category.as_deref() {
                Some("omo") => Some((&crate::services::omo::STANDARD, &crate::services::omo::SLIM)),
                Some("omo-slim") => {
                    Some((&crate::services::omo::SLIM, &crate::services::omo::STANDARD))
                }
                _ => None,
            };
            if let Some((enable, disable)) = omo_pair {
                crate::services::OmoService::activate_provider_locked(state, provider, enable)?;
                let mut result = SwitchResult::default();
                if let Err(err) = crate::services::OmoService::delete_config_file(disable) {
                    log::warn!("切到 {id} 后删除另一个 OMO 变体的配置失败: {err}");
                    result
                        .warnings
                        .push(format!("omo_variant_cleanup_failed:{id}"));
                }
                return Ok(result);
            }
        }

        let result = SwitchResult::default();

        // Additive mode apps skip setting is_current (no such concept).
        if !app_type.is_additive_mode() {
            crate::settings::set_current_provider(&app_type, Some(id))?;
            state.db.set_current_provider(&app_type, id)?;
        }

        // 写 live（Claude Desktop、累加式应用；切换式应用在上面各自的分支里写完了）。
        write_live_for_state(state, &app_type, provider)?;

        // Hermes is additive, so "switching" doesn't overwrite a live config file
        // — we instead update the top-level `model:` section to point at this
        // provider's first declared model. Without this, clicking "switch" would
        // only shuffle entries in custom_providers[] while Hermes keeps using
        // whatever `model.provider` was set before.

        // For additive-mode providers that were DB-only (live_config_managed == Some(false)),
        // flip the flag to true now that the provider has been successfully written to the live
        // file. This ensures sync_all_providers_to_live() will include it on future syncs.
        //
        // If persisting the marker fails, roll back the just-written live config so we don't leave
        // the provider in a silent inconsistent state (present in live, but still marked DB-only).
        if app_type.is_additive_mode() && Self::provider_live_config_managed(provider) != Some(true)
        {
            let mut updated = provider.clone();
            Self::set_provider_live_config_managed(&mut updated, true);
            if let Err(e) = state.db.save_provider(&app_type, &updated) {
                let rollback_result = match app_type {
                    AppType::OpenCode => remove_opencode_provider_from_live(&provider.id),

                    AppType::Dsh => crate::dsh_config::remove_provider(&provider.id),
                    _ => Ok(()),
                };

                match rollback_result {
                    Ok(()) => {
                        return Err(AppError::Message(format!(
                            "Failed to persist live_config_managed for '{}' after writing live config; live changes were rolled back: {e}",
                            provider.id
                        )));
                    }
                    Err(rollback_err) => {
                        return Err(AppError::Message(format!(
                            "Failed to persist live_config_managed for '{}' after writing live config: {e}; additionally failed to roll back live config: {rollback_err}",
                            provider.id
                        )));
                    }
                }
            }
        }

        // 切换重写了目标应用的 live，只重投影该应用的 MCP（Grok Build 的
        // [mcp_servers] 与 live 同文件，整体替换后必须补回；其余应用的
        // MCP 文件独立于 live，投影是幂等维护）。不用全量 sync_all_enabled：
        // 无关应用的 live 损坏（如 ~/.claude.json 坏 JSON）不该阻断切换。
        // 走到这里 DB is_current 与 live 都已落盘，切换事实上已成功；
        // 投影失败上抛会让前端报"切换失败"制造分裂假象，故降级为警告
        // （MCP 投影可自愈：下次切换 / 任一 MCP 启停都会重新投影）。
        if let Err(err) = McpService::sync_enabled_for_app(state, &app_type) {
            log::warn!("切换供应商后重投影 {app_type:?} MCP 失败（将在下次同步时自愈）: {err}");
        }

        Ok(result)
    }

    /// Claude Code 直连切换：只替换关键字段和独有字段，文件和指针在同一个操作里提交。
    ///
    /// 不回填、不同步通用配置片段：用户在 live 里的改动本来就留在原处。发布前的失败
    /// （解析不了、并发冲突）什么都不改；开始发布后由 pending 保证前滚补完。
    fn switch_claude_direct(
        state: &AppState,
        provider: &Provider,
        providers: &IndexMap<String, Provider>,
    ) -> Result<SwitchResult, AppError> {
        let current_id = crate::mode::current::provider_for(
            &state.db,
            &AppType::Claude,
            crate::mode::current::Purpose::Direct,
        )?;
        let prev = current_id
            .as_deref()
            .and_then(|current_id| providers.get(current_id));
        claude_direct::switch_to(state.db.as_ref(), prev, provider)?;

        // MCP 在 ~/.claude.json，和 settings.json 无关；重投影是幂等维护，失败只记警告
        // （切换已经提交，下次同步会自愈）。
        if let Err(err) = McpService::sync_enabled_for_app(state, &AppType::Claude) {
            log::warn!("切换供应商后重投影 claude MCP 失败（将在下次同步时自愈）: {err}");
        }
        Ok(SwitchResult::default())
    }

    /// Codex 直连切换：`config.toml` 只替换关键字段和独有字段；`auth.json`、模型目录、
    /// 托管账号标记和指针在同一个操作里提交。
    ///
    /// 不回填、不同步通用配置片段、不补回 MCP：用户在 live 里的改动（含 `[mcp_servers]`）
    /// 本来就留在原处。行有问题（会把官方登录发给第三方、带 Key 却没地方放）时在写任何
    /// 东西之前报错，指针也不动。
    fn switch_codex_direct(
        state: &AppState,
        provider: &Provider,
        providers: &IndexMap<String, Provider>,
    ) -> Result<SwitchResult, AppError> {
        let current_id = crate::mode::current::provider_for(
            &state.db,
            &AppType::Codex,
            crate::mode::current::Purpose::Direct,
        )?;
        let owner = current_id
            .as_deref()
            .and_then(|current_id| providers.get(current_id))
            .map_or(codex_direct::Owner::None, codex_direct::Owner::Provider);
        codex_direct::write_direct(
            state.db.as_ref(),
            &state.codex_oauth_manager,
            crate::mode::state::op::SWITCH,
            owner,
            Some(provider),
            crate::mode::state::PendingTarget::pointer(Some(provider.id.clone())),
        )?;

        let mut result = SwitchResult::default();
        // 保留登录关闭时切到第三方要删掉 auth.json。删不掉（只读目录、被占用）不让切换
        // 失败：配置和指针都已提交，但要让用户看到官方登录还在盘上。
        if !codex_direct::is_official(provider)
            && !crate::settings::preserve_codex_official_auth_on_switch()
            && crate::codex_config::get_codex_auth_path().exists()
        {
            log::warn!("Codex auth.json still present after a preservation-off third-party switch");
            result
                .warnings
                .push("codex_auth_cleanup_failed".to_string());
        }
        Ok(result)
    }

    /// Sync current provider to live configuration (re-export)
    pub fn sync_current_to_live(state: &AppState) -> Result<(), AppError> {
        sync_current_to_live(state)
    }

    pub fn sync_current_provider_for_app(
        state: &AppState,
        app_type: AppType,
    ) -> Result<(), AppError> {
        if app_type.is_additive_mode() {
            return sync_additive_app_to_live(state, &app_type);
        }
        // 没有正在用的那家、或者在代理模式（客户端文件没按直连重写）时不重投影 MCP。
        let outcome = live::sync_current_provider_for_app_respecting_mode(state, &app_type)?;
        if outcome != Some(LiveSyncOutcome::WroteLive) {
            return Ok(());
        }
        McpService::sync_enabled_for_app(state, &app_type)
    }

    pub fn migrate_legacy_common_config_usage(
        state: &AppState,
        app_type: AppType,
        legacy_snippet: &str,
    ) -> Result<(), AppError> {
        if app_type.is_additive_mode() || legacy_snippet.trim().is_empty() {
            return Ok(());
        }

        let providers = state.db.get_all_providers(&app_type)?;

        for provider in providers.values() {
            if provider
                .meta
                .as_ref()
                .and_then(|meta| meta.common_config_enabled)
                .is_some()
            {
                continue;
            }

            if !live::provider_uses_common_config(&app_type, provider, Some(legacy_snippet)) {
                continue;
            }

            let mut updated_provider = provider.clone();
            updated_provider
                .meta
                .get_or_insert_with(Default::default)
                .common_config_enabled = Some(true);

            match live::remove_common_config_from_settings(
                &app_type,
                &updated_provider.settings_config,
                legacy_snippet,
            ) {
                Ok(settings) => updated_provider.settings_config = settings,
                Err(err) => {
                    log::warn!(
                        "Failed to normalize legacy common config for {} provider '{}': {err}",
                        app_type.as_str(),
                        updated_provider.id
                    );
                }
            }

            state.db.save_provider(&app_type, &updated_provider)?;
        }

        Ok(())
    }

    pub fn migrate_legacy_common_config_usage_if_needed(
        state: &AppState,
        app_type: AppType,
    ) -> Result<(), AppError> {
        if app_type.is_additive_mode() {
            return Ok(());
        }

        let Some(snippet) = state.db.get_config_snippet(app_type.as_str())? else {
            return Ok(());
        };

        if snippet.trim().is_empty() {
            return Ok(());
        }

        Self::migrate_legacy_common_config_usage(state, app_type, &snippet)
    }

    /// Extract common config snippet from current provider
    ///
    /// Extracts the current provider's configuration and removes provider-specific fields
    /// (API keys, model settings, endpoints) to create a reusable common config snippet.
    pub fn extract_common_config_snippet(
        state: &AppState,
        app_type: AppType,
    ) -> Result<String, AppError> {
        // Get current provider
        let current_id = Self::current(state, app_type.clone())?;
        if current_id.is_empty() {
            return Err(AppError::Message("No current provider".to_string()));
        }

        let providers = state.db.get_all_providers(&app_type)?;
        let provider = providers
            .get(&current_id)
            .ok_or_else(|| AppError::Message(format!("Provider {current_id} not found")))?;

        match app_type {
            AppType::Claude => Self::extract_claude_common_config(&provider.settings_config),

            AppType::Codex => Self::extract_codex_common_config(&provider.settings_config),

            AppType::OpenCode => Self::extract_opencode_common_config(&provider.settings_config),

            // Hermes doesn't use common config snippets
            AppType::Dsh | AppType::Agy => Ok(String::new()),
        }
    }

    /// Extract common config snippet from a config value (e.g. editor content).
    pub fn extract_common_config_snippet_from_settings(
        app_type: AppType,
        settings_config: &Value,
    ) -> Result<String, AppError> {
        match app_type {
            AppType::Claude => Self::extract_claude_common_config(settings_config),

            AppType::Codex => Self::extract_codex_common_config(settings_config),

            AppType::OpenCode => Self::extract_opencode_common_config(settings_config),

            // Hermes doesn't use common config snippets
            AppType::Dsh | AppType::Agy => Ok(String::new()),
        }
    }

    /// 判断一个 env / 顶层配置键名是否为凭据/机密：凡命中一律不得写入共享的
    /// 通用配置片段。**故意从严**——多剥一个非机密键只是它不被共享（可恢复的小
    /// 不便），漏剥一个凭据则会把密钥注入到每个供应商（不可恢复的泄漏）。因此用
    /// 模式匹配覆盖整类，而非枚举具体名字（枚举永远会漏掉下一个 `*_API_KEY`）。
    ///
    /// 覆盖：Anthropic / OpenRouter / Google / OpenAI / Gemini 等 `*_API_KEY`
    /// （Claude provider 的凭据见 `Provider::resolve_usage_credentials`，确实支持
    /// `OPENROUTER_API_KEY` / `GOOGLE_API_KEY` 等回退）、各类 `*_AUTH_TOKEN` /
    /// 单数 `*_TOKEN`、AWS Bedrock / Vertex 凭据、通用 secret / password /
    /// 私钥命名，以及发往上游的自定义请求头、Cookie、Authorization。
    pub fn is_sensitive_config_key(name: &str) -> bool {
        let upper = name.to_ascii_uppercase();

        // 单数 `_TOKEN` 命中 AWS_SESSION_TOKEN 等，但**不**误伤复数 `_TOKENS`
        // （CLAUDE_CODE_MAX_OUTPUT_TOKENS / MAX_THINKING_TOKENS 是正常可共享配置）。
        const SENSITIVE_SUFFIXES: &[&str] = &[
            // 必须单列：只枚举 `_API_KEY` / `_ACCESS_KEY` 这些子类，等于把最普通
            // 的那一种漏在外面。下面几条 `_*_KEY` 被它蕴含，保留是为了说明覆盖面。
            "_KEY",
            "_API_KEY",
            "_ACCESS_KEY",
            "_ACCESS_KEY_ID",
            "_KEY_ID",
            "_PRIVATE_KEY",
            // 不带分隔符的复合写法各走各的后缀：`_KEY` 够不着 `..._APIKEY`
            // （倒数第四个字符是 I 不是下划线）。VOLC_ACCESSKEY 是火山引擎文档
            // 里的正式变量名，本仓库就实现了火山 AK/SK 用量查询。
            "_APIKEY",
            "_ACCESSKEY",
            "_SECRETKEY",
            "_APITOKEN",
            "_AUTH_TOKEN",
            "_TOKEN",
            // GITHUB_PAT / GITLAB_PAT 等 personal access token 的惯用写法，
            // 既不含 TOKEN 也不含 KEY，前面每一条规则都够不着。
            "_PAT",
            // 口令类的常见缩写。`_PASS` 不会误伤 `*_BYPASS`（那个以 `_BYPASS`
            // 结尾），`_PWD` 也不会误伤 shell 的 PWD / OLDPWD。
            "_PWD",
            "_PASS",
            "_PASSPHRASE",
            "_CREDS",
            // 发往上游的自定义请求头（ANTHROPIC_CUSTOM_HEADERS、
            // GEMINI_CLI_CUSTOM_HEADERS）：常见写法是 `Authorization: Bearer …`
            // 或 `Cookie: …`，整串就是凭据。只认 `_CUSTOM_HEADERS`，不按 HEADER
            // 一刀切：CLAUDE_CODE_ATTRIBUTION_HEADER 是普通开关，
            // OTEL_EXPORTER_OTLP_HEADERS 发往用户自己的遥测端点，都应照常共享。
            "_CUSTOM_HEADERS",
        ];
        const SENSITIVE_EXACT: &[&str] = &[
            "APIKEY",
            "API_KEY",
            "TOKEN",
            "SECRET",
            "PASSWORD",
            "CREDENTIALS",
            "HEADERS",
        ];
        // contains：覆盖 AWS_SECRET_ACCESS_KEY / *_CLIENT_SECRET /
        // GOOGLE_APPLICATION_CREDENTIALS / AWS_BEARER_TOKEN_BEDROCK 等变体。
        const SENSITIVE_CONTAINS: &[&str] = &[
            "SECRET",
            "PASSWORD",
            "PASSWD",
            "CREDENTIAL",
            "PRIVATE_KEY",
            "BEARER_TOKEN",
            "COOKIE",
            "AUTHORIZATION",
        ];

        SENSITIVE_EXACT.contains(&upper.as_str())
            || SENSITIVE_SUFFIXES.iter().any(|s| upper.ends_with(s))
            || SENSITIVE_CONTAINS.iter().any(|c| upper.contains(c))
    }

    /// Claude `env` 里的键能否进通用配置片段。
    ///
    /// 片段会合并进每一家勾选了它的供应商，所以只能放与供应商无关的设置。关键字段
    /// （请求发到哪、凭什么鉴权、哪个模型名、哪种协议）一旦进了片段，就会跟着切换带给
    /// 下一家：从 Bedrock 切到官方后，官方的 live 里还留着 `CLAUDE_CODE_USE_BEDROCK=1`，
    /// Claude Code 继续走 Bedrock。凭据另由 `is_sensitive_config_key` 统一剥离。
    fn claude_env_key_is_shared(key: &str) -> bool {
        // Context limits follow the actual upstream model. Sharing these
        // across providers can cap GPT/Kimi to the wrong window and make
        // Claude Code compact too early or miss the upstream limit.
        const UPSTREAM_WINDOW_KEYS: &[&str] = &[
            "CLAUDE_CODE_MAX_CONTEXT_TOKENS",
            "CLAUDE_CODE_AUTO_COMPACT_WINDOW",
        ];

        !crate::live::floor::claude_floor_env(key)
            && !UPSTREAM_WINDOW_KEYS.contains(&key)
            && !Self::is_sensitive_config_key(key)
    }

    /// Claude 顶层键能否进通用配置片段，口径同 [`Self::claude_env_key_is_shared`]。
    /// `model` 是 `/model` 保存的选择，属于当时那一家。
    fn claude_top_key_is_shared(key: &str) -> bool {
        !crate::live::floor::claude_floor_top(key) && !Self::is_sensitive_config_key(key)
    }

    /// Extract common config for Claude (JSON format)
    fn extract_claude_common_config(settings: &Value) -> Result<String, AppError> {
        let mut config = settings.clone();

        if let Some(obj) = config.as_object_mut() {
            if let Some(Value::Object(env)) = obj.get_mut("env") {
                env.retain(|key, _| Self::claude_env_key_is_shared(key));
            }
            obj.retain(|key, value| match key.as_str() {
                "env" => !value.as_object().is_some_and(|env| env.is_empty()),
                _ => Self::claude_top_key_is_shared(key),
            });
        }

        // Check if result is empty
        if config.as_object().is_none_or(|obj| obj.is_empty()) {
            return Ok("{}".to_string());
        }

        serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))
    }

    /// Extract common config for Codex (TOML format)
    fn extract_codex_common_config(settings: &Value) -> Result<String, AppError> {
        // Codex config is stored as { "auth": {...}, "config": "toml string" }
        let config_toml = settings
            .get("config")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if config_toml.is_empty() {
            return Ok(String::new());
        }

        let mut doc = config_toml
            .parse::<toml_edit::DocumentMut>()
            .map_err(|e| AppError::Message(format!("TOML parse error: {e}")))?;

        // 关键字段（选路、模型名、推理档位等，见 `live::floor`）归供应商，不进片段：
        // 片段已冻结，收进去的值会从行里被迁移剥掉、再也写不回 live。
        let root = doc.as_table_mut();
        for key in crate::live::floor::CODEX_FLOOR_TOP {
            root.remove(key);
        }
        for path in crate::live::floor::CODEX_FLOOR_NESTED {
            let [parent, key] = path else { continue };
            if let Some(table) = root
                .get_mut(parent)
                .and_then(|item| item.as_table_like_mut())
            {
                table.remove(key);
                if table.is_empty() {
                    root.remove(parent);
                }
            }
        }

        // Remove entire model_providers table (provider-specific configuration)
        root.remove("model_providers");

        // MCP 服务器归 DB mcp_servers 表所有：进了共享片段会绕过按应用的
        // 启用状态被合并进所有勾选通用配置的供应商，且在通用配置编辑框里
        // 显示为一份"重复"的 MCP 配置。
        root.remove("mcp_servers");
        // 历史错误格式 [mcp.servers] 一并剥离（与 strip_codex_mcp_servers_from_settings
        // 一致）：sync_all_enabled 只管理 [mcp_servers.*]，legacy 形态一旦进了
        // 片段就会被合并进所有供应商，且没有任何同步路径能清掉这个孤儿。
        if let Some(mcp_tbl) = root
            .get_mut("mcp")
            .and_then(|item| item.as_table_like_mut())
        {
            mcp_tbl.remove("servers");
            if mcp_tbl.is_empty() {
                root.remove("mcp");
            }
        }

        // cc-switch 写 live 时注入的产物一律不进共享片段：
        // - experimental_bearer_token 正常写在 [model_providers.<id>] 内（上面
        //   整表已剥），但无活跃路由 / 内建保留 id / 路由表缺失三种 fallback
        //   会落在顶层——不剥等于把 API 密钥写进共享片段。
        root.remove("experimental_bearer_token");
        // - model_catalog_json 指向按供应商生成的 catalog 投影文件（DB 为 SSOT）。
        root.remove("model_catalog_json");
        // - web_search 只剥 cc-switch 注入的 "disabled" 哨兵；用户手设的其它值
        //   属于可共享偏好，保留。
        if root
            .get(crate::codex_config::CODEX_WEB_SEARCH_FIELD)
            .and_then(|item| item.as_str())
            == Some(crate::codex_config::CODEX_WEB_SEARCH_DISABLED)
        {
            root.remove(crate::codex_config::CODEX_WEB_SEARCH_FIELD);
        }

        // Clean up multiple empty lines (keep at most one blank line).
        let mut cleaned = String::new();
        let mut blank_run = 0usize;
        for line in doc.to_string().lines() {
            if line.trim().is_empty() {
                blank_run += 1;
                if blank_run <= 1 {
                    cleaned.push('\n');
                }
                continue;
            }
            blank_run = 0;
            cleaned.push_str(line);
            cleaned.push('\n');
        }

        Ok(cleaned.trim().to_string())
    }

    /// Extract common config for OpenCode (JSON format)
    fn extract_opencode_common_config(settings: &Value) -> Result<String, AppError> {
        // OpenCode uses a different config structure with npm, options, models
        // For common config, we exclude provider-specific fields like apiKey
        let mut config = settings.clone();

        // Remove provider-specific fields
        if let Some(obj) = config.as_object_mut() {
            for key in ["options", "settings"] {
                if let Some(options) = obj.get_mut(key).and_then(|v| v.as_object_mut()) {
                    options.remove("apiKey");
                    options.remove("baseURL");
                }
            }
            // Keep npm and models as they might be common
        }

        if config.is_null() || (config.is_object() && config.as_object().unwrap().is_empty()) {
            return Ok("{}".to_string());
        }

        serde_json::to_string_pretty(&config)
            .map_err(|e| AppError::Message(format!("Serialization failed: {e}")))
    }

    /// Import default configuration from live files (re-export)
    ///
    /// Returns `Ok(true)` if imported, `Ok(false)` if skipped.
    pub fn import_default_config(state: &AppState, app_type: AppType) -> Result<bool, AppError> {
        import_default_config(state, app_type)
    }

    pub fn should_import_default_config_on_startup(
        state: &AppState,
        app_type: &AppType,
    ) -> Result<bool, AppError> {
        should_import_default_config_on_startup(state, app_type)
    }

    /// Read current live settings (re-export)
    pub fn read_live_settings(app_type: AppType) -> Result<Value, AppError> {
        read_live_settings(app_type)
    }

    /// Get custom endpoints list (re-export)
    pub fn get_custom_endpoints(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
    ) -> Result<Vec<CustomEndpoint>, AppError> {
        endpoints::get_custom_endpoints(state, app_type, provider_id)
    }

    /// Add custom endpoint (re-export)
    pub fn add_custom_endpoint(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        url: String,
    ) -> Result<(), AppError> {
        endpoints::add_custom_endpoint(state, app_type, provider_id, url)
    }

    /// Remove custom endpoint (re-export)
    pub fn remove_custom_endpoint(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        url: String,
    ) -> Result<(), AppError> {
        endpoints::remove_custom_endpoint(state, app_type, provider_id, url)
    }

    /// Update endpoint last used timestamp (re-export)
    pub fn update_endpoint_last_used(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        url: String,
    ) -> Result<(), AppError> {
        endpoints::update_endpoint_last_used(state, app_type, provider_id, url)
    }

    /// Update provider sort order
    pub fn update_sort_order(
        state: &AppState,
        app_type: AppType,
        updates: Vec<ProviderSortUpdate>,
    ) -> Result<bool, AppError> {
        let updates: Vec<(String, usize)> = updates
            .into_iter()
            .map(|update| (update.id, update.sort_index))
            .collect();
        state.db.update_provider_sort_indexes(&app_type, &updates)?;
        Ok(true)
    }

    /// Query provider usage (re-export)
    pub async fn query_usage(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
    ) -> Result<UsageResult, AppError> {
        usage::query_usage(state, app_type, provider_id).await
    }

    /// Test usage script (re-export)
    #[allow(clippy::too_many_arguments)]
    pub async fn test_usage_script(
        state: &AppState,
        app_type: AppType,
        provider_id: &str,
        script_code: &str,
        timeout: u64,
        api_key: Option<&str>,
        base_url: Option<&str>,
        access_token: Option<&str>,
        user_id: Option<&str>,
        template_type: Option<&str>,
    ) -> Result<UsageResult, AppError> {
        usage::test_usage_script(
            state,
            app_type,
            provider_id,
            script_code,
            timeout,
            api_key,
            base_url,
            access_token,
            user_id,
            template_type,
        )
        .await
    }

    fn validate_provider_settings(app_type: &AppType, provider: &Provider) -> Result<(), AppError> {
        // 校验发生在任何 live 文件写入之前，不能仅依赖保存数据库时拒绝。
        provider.validate_service_support()?;
        icon::validate_icon(provider.icon.as_deref())?;
        match app_type {
            AppType::Claude => {
                if !provider.settings_config.is_object() {
                    return Err(AppError::localized(
                        "provider.claude.settings.not_object",
                        "Claude 配置必须是 JSON 对象",
                        "Claude configuration must be a JSON object",
                    ));
                }
            }

            AppType::Codex => {
                let settings = provider.settings_config.as_object().ok_or_else(|| {
                    AppError::localized(
                        "provider.codex.settings.not_object",
                        "Codex 配置必须是 JSON 对象",
                        "Codex configuration must be a JSON object",
                    )
                })?;

                let auth = settings.get("auth").ok_or_else(|| {
                    AppError::localized(
                        "provider.codex.auth.missing",
                        format!("供应商 {} 缺少 auth 配置", provider.id),
                        format!("Provider {} is missing auth configuration", provider.id),
                    )
                })?;
                if !auth.is_object() {
                    return Err(AppError::localized(
                        "provider.codex.auth.not_object",
                        format!("供应商 {} 的 auth 配置必须是 JSON 对象", provider.id),
                        format!(
                            "Provider {} auth configuration must be a JSON object",
                            provider.id
                        ),
                    ));
                }

                if let Some(config_value) = settings.get("config") {
                    if !(config_value.is_string() || config_value.is_null()) {
                        return Err(AppError::localized(
                            "provider.codex.config.invalid_type",
                            "Codex config 字段必须是字符串",
                            "Codex config field must be a string",
                        ));
                    }
                    if let Some(cfg_text) = config_value.as_str() {
                        crate::codex_config::validate_config_toml(cfg_text)?;
                    }
                }
            }

            AppType::Agy => crate::agy_config::validate_settings(&provider.settings_config)?,

            AppType::OpenCode => {
                // OpenCode uses a different config structure: { npm, options, models }
                // Basic validation - must be an object
                if !provider.settings_config.is_object() {
                    return Err(AppError::localized(
                        "provider.opencode.settings.not_object",
                        "OpenCode 配置必须是 JSON 对象",
                        "OpenCode configuration must be a JSON object",
                    ));
                }
                // The row is saved before the live write, which would refuse an invalid
                // native declaration only after the database already holds it.
                if !matches!(provider.category.as_deref(), Some("omo") | Some("omo-slim")) {
                    let (fragment, format) = crate::opencode_config::provider_fragment(
                        &provider.id,
                        &provider.settings_config,
                        provider.opencode_config_format(),
                    )?;
                    if format == crate::provider::OpenCodeConfigFormat::V2 {
                        crate::opencode_config::validate_native_provider(&provider.id, fragment)?;
                    }
                }
            }

            AppType::Dsh => {
                crate::dsh_config::validate_provider(&provider.id, &provider.settings_config)?;
            }
        }

        // Validate and clean UsageScript configuration (common for all app types)
        if let Some(meta) = &provider.meta {
            if let Some(usage_script) = &meta.usage_script {
                validate_usage_script(usage_script)?;
            }
        }

        Ok(())
    }
}

/// Normalize Claude model keys in a JSON value
///
/// Reads old key (ANTHROPIC_SMALL_FAST_MODEL), writes new keys (DEFAULT_*), and deletes old key.
pub fn normalize_claude_models_in_value(settings: &mut Value) -> bool {
    let mut changed = false;
    let env = match settings.get_mut("env").and_then(|v| v.as_object_mut()) {
        Some(obj) => obj,
        None => return changed,
    };

    let model = env
        .get("ANTHROPIC_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let small_fast = env
        .get("ANTHROPIC_SMALL_FAST_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let current_haiku = env
        .get("ANTHROPIC_DEFAULT_HAIKU_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let current_sonnet = env
        .get("ANTHROPIC_DEFAULT_SONNET_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());
    let current_opus = env
        .get("ANTHROPIC_DEFAULT_OPUS_MODEL")
        .and_then(|v| v.as_str())
        .map(|s| s.to_string());

    let target_haiku = current_haiku
        .or_else(|| small_fast.clone())
        .or_else(|| model.clone());
    let target_sonnet = current_sonnet
        .or_else(|| model.clone())
        .or_else(|| small_fast.clone());
    let target_opus = current_opus
        .or_else(|| model.clone())
        .or_else(|| small_fast.clone());

    if env.get("ANTHROPIC_DEFAULT_HAIKU_MODEL").is_none() {
        if let Some(v) = target_haiku {
            env.insert(
                "ANTHROPIC_DEFAULT_HAIKU_MODEL".to_string(),
                Value::String(v),
            );
            changed = true;
        }
    }
    if env.get("ANTHROPIC_DEFAULT_SONNET_MODEL").is_none() {
        if let Some(v) = target_sonnet {
            env.insert(
                "ANTHROPIC_DEFAULT_SONNET_MODEL".to_string(),
                Value::String(v),
            );
            changed = true;
        }
    }
    if env.get("ANTHROPIC_DEFAULT_OPUS_MODEL").is_none() {
        if let Some(v) = target_opus {
            env.insert("ANTHROPIC_DEFAULT_OPUS_MODEL".to_string(), Value::String(v));
            changed = true;
        }
    }

    if env.remove("ANTHROPIC_SMALL_FAST_MODEL").is_some() {
        changed = true;
    }

    changed
}

#[derive(Debug, Clone, Deserialize)]
pub struct ProviderSortUpdate {
    pub id: String,
    #[serde(rename = "sortIndex")]
    pub sort_index: usize,
}

// ============================================================================
// 统一供应商（Universal Provider）服务方法
// ============================================================================

use crate::provider::UniversalProvider;
use std::collections::HashMap;

impl ProviderService {
    /// 获取所有统一供应商
    pub fn list_universal(
        state: &AppState,
    ) -> Result<HashMap<String, UniversalProvider>, AppError> {
        state.db.get_all_universal_providers()
    }

    /// 获取单个统一供应商
    pub fn get_universal(
        state: &AppState,
        id: &str,
    ) -> Result<Option<UniversalProvider>, AppError> {
        state.db.get_universal_provider(id)
    }

    /// 添加或更新统一供应商（不自动同步，需手动调用 sync_universal_to_apps）
    pub fn upsert_universal(
        state: &AppState,
        provider: UniversalProvider,
    ) -> Result<bool, AppError> {
        icon::validate_icon(provider.icon.as_deref())?;
        // 保存统一供应商
        state.db.save_universal_provider(&provider)?;

        Ok(true)
    }

    /// 删除统一供应商
    pub fn delete_universal(state: &AppState, id: &str) -> Result<bool, AppError> {
        // 按稳定生成 ID 查找实际存在的子供应商，而不是只看当前 apps flags。
        // 否则「曾同步 → 关闭某应用仅保存 → 删除父项」会留下历史子项。
        let child_targets = [
            (AppType::Claude, format!("universal-claude-{id}")),
            (AppType::Codex, format!("universal-codex-{id}")),
        ];
        let existing_children = {
            let mut found = Vec::new();
            for (app, child_id) in &child_targets {
                if state.db.provider_exists(child_id, app)? {
                    found.push((app.clone(), child_id.clone()));
                }
            }
            found
        };

        Self::ensure_universal_children_deletable(state, &existing_children)?;
        for (app, child_id) in &existing_children {
            Self::delete(state, app.clone(), child_id)?;
        }

        state.db.delete_universal_provider(id)?;
        Ok(true)
    }

    fn ensure_universal_children_deletable(
        state: &AppState,
        children: &[(AppType, String)],
    ) -> Result<(), AppError> {
        let mut blocking = Vec::new();
        for (app, child_id) in children {
            if crate::mode::current::is_referenced(&state.db, app, child_id)? {
                blocking.push(format!("{} ({})", child_id, app.as_str()));
            }
        }
        if blocking.is_empty() {
            return Ok(());
        }
        Err(AppError::Message(format!(
            "无法删除统一供应商：以下子供应商正在使用中：{}",
            blocking.join("、")
        )))
    }

    /// 同步统一供应商到各应用
    pub fn sync_universal_to_apps(state: &AppState, id: &str) -> Result<bool, AppError> {
        let provider = state
            .db
            .get_universal_provider(id)?
            .ok_or_else(|| AppError::Message(format!("统一供应商 {id} 不存在")))?;
        // 子供应商经 save_provider 直接入库，不走保存校验，图标在这里把关
        icon::validate_icon(provider.icon.as_deref())?;

        // Reject disabled children that are in use before updating any sibling.
        let disabled = [
            (AppType::Claude, provider.apps.claude),
            (AppType::Codex, provider.apps.codex),
        ]
        .into_iter()
        .filter(|(_, enabled)| !enabled)
        .map(|(app, _)| {
            let child_id = format!("universal-{}-{id}", app.as_str());
            (app, child_id)
        })
        .collect::<Vec<_>>();
        Self::ensure_universal_children_deletable(state, &disabled)?;

        // Keep DB and live projections in sync independently per application:
        // one broken config file must not prevent the other two apps from being
        // updated, but it must still be reported instead of returning success.
        let mut live_failures = Vec::new();

        // 同步到 Claude
        if let Some(mut claude_provider) = provider.to_claude_provider() {
            let _guard = crate::mode::controller::lock_settled_blocking(state, &AppType::Claude)?;
            // 合并已有配置
            if let Some(existing) = state
                .db
                .get_provider_by_id(&claude_provider.id, &AppType::Claude)?
            {
                // Updating the shared key must retain the child's selected auth
                // header. Adding AUTH_TOKEN to an API_KEY child changes it to
                // Bearer authentication and leaves the old credential behind.
                let api_key_auth = existing
                    .meta
                    .as_ref()
                    .and_then(|meta| meta.api_key_field.as_deref())
                    .map(|field| field == "ANTHROPIC_API_KEY")
                    .unwrap_or_else(|| {
                        let env = &existing.settings_config["env"];
                        env.get("ANTHROPIC_API_KEY").is_some()
                            && env.get("ANTHROPIC_AUTH_TOKEN").is_none()
                    });
                let mut merged = existing.settings_config.clone();
                Self::merge_json(&mut merged, &claude_provider.settings_config);
                if let Some(env) = merged["env"].as_object_mut() {
                    let (selected, other) = if api_key_auth {
                        ("ANTHROPIC_API_KEY", "ANTHROPIC_AUTH_TOKEN")
                    } else {
                        ("ANTHROPIC_AUTH_TOKEN", "ANTHROPIC_API_KEY")
                    };
                    env.remove(other);
                    env.insert(selected.into(), Value::String(provider.api_key.clone()));
                }
                claude_provider.settings_config = merged;
                // 已有子供应商的应用专属配置与排序不属于统一供应商管理的字段。
                claude_provider.meta = existing.meta;
                claude_provider.created_at = existing.created_at;
                claude_provider.sort_index = existing.sort_index;
                claude_provider.in_failover_queue = existing.in_failover_queue;
            }
            state.db.save_provider(&AppType::Claude, &claude_provider)?;
            Self::project_universal_child_to_live(
                state,
                AppType::Claude,
                &claude_provider.id,
                &mut live_failures,
            );
        } else {
            // 如果禁用了 Claude，走普通删除（含引用检查与切换锁）。
            let claude_id = format!("universal-claude-{id}");
            if state
                .db
                .get_provider_by_id(&claude_id, &AppType::Claude)?
                .is_some()
                || state
                    .db
                    .get_unreadable_provider(&AppType::Claude, &claude_id)?
                    .is_some()
            {
                Self::delete(state, AppType::Claude, &claude_id)?;
            }
        }

        // 同步到 Codex
        if let Some(mut codex_provider) = provider.to_codex_provider() {
            let _guard = crate::mode::controller::lock_settled_blocking(state, &AppType::Codex)?;
            // 合并已有配置
            if let Some(existing) = state
                .db
                .get_provider_by_id(&codex_provider.id, &AppType::Codex)?
            {
                let mut merged = existing.settings_config.clone();
                Self::merge_json(&mut merged, &codex_provider.settings_config);
                if let Some(config) = existing
                    .settings_config
                    .get("config")
                    .and_then(Value::as_str)
                {
                    let mut document =
                        config.parse::<toml_edit::DocumentMut>().map_err(|error| {
                            AppError::Config(format!("Invalid existing Codex config: {error}"))
                        })?;
                    if let Some(providers) = document.get("model_providers") {
                        let providers = providers.as_table_like().ok_or_else(|| {
                            AppError::Config("Codex model_providers must be a table".into())
                        })?;
                        if providers
                            .get("custom")
                            .is_some_and(|item| item.as_table_like().is_none())
                        {
                            return Err(AppError::Config(
                                "Codex model_providers.custom must be a table".into(),
                            ));
                        }
                    }
                    let generated = codex_provider.settings_config["config"]
                        .as_str()
                        .unwrap_or_default()
                        .parse::<toml_edit::DocumentMut>()
                        .map_err(|error| {
                            AppError::Config(format!("Invalid generated Codex config: {error}"))
                        })?;
                    for key in ["model_provider", "model", "model_reasoning_effort"] {
                        document[key] = generated[key].clone();
                    }
                    for key in ["name", "base_url", "wire_api", "requires_openai_auth"] {
                        document["model_providers"]["custom"][key] =
                            generated["model_providers"]["custom"][key].clone();
                    }
                    merged["config"] = Value::String(document.to_string());
                }
                codex_provider.settings_config = merged;
                // 已有子供应商的应用专属配置与排序不属于统一供应商管理的字段。
                codex_provider.meta = existing.meta;
                codex_provider.created_at = existing.created_at;
                codex_provider.sort_index = existing.sort_index;
                codex_provider.in_failover_queue = existing.in_failover_queue;
            }
            state.db.save_provider(&AppType::Codex, &codex_provider)?;
            Self::project_universal_child_to_live(
                state,
                AppType::Codex,
                &codex_provider.id,
                &mut live_failures,
            );
        } else {
            let codex_id = format!("universal-codex-{id}");
            if state
                .db
                .get_provider_by_id(&codex_id, &AppType::Codex)?
                .is_some()
                || state
                    .db
                    .get_unreadable_provider(&AppType::Codex, &codex_id)?
                    .is_some()
            {
                Self::delete(state, AppType::Codex, &codex_id)?;
            }
        }

        // Retired child rows remain readable; syncing supported apps does not rewrite them.

        if live_failures.is_empty() {
            Ok(true)
        } else {
            Err(AppError::Message(format!(
                "统一供应商已保存到数据库，但以下应用的配置文件未能写入，仍是旧内容：{}。请重试同步，或切换一次该应用的供应商。",
                live_failures.join("、")
            )))
        }
    }

    /// Re-project a generated universal child only when it is the effective
    /// current provider for that app. Failures are collected by the caller so
    /// the other applications can continue syncing.
    fn project_universal_child_to_live(
        state: &AppState,
        app_type: AppType,
        child_id: &str,
        failures: &mut Vec<String>,
    ) {
        // 正在用的那家（代理模式下是代理路由）才需要重投影。
        let is_current = match crate::mode::current::provider_for(
            &state.db,
            &app_type,
            crate::mode::current::Purpose::InUse,
        ) {
            Ok(current) => current.as_deref() == Some(child_id),
            Err(err) => {
                log::warn!(
                    "读取 {} 当前供应商失败，跳过统一供应商的 live 重投影: {err}",
                    app_type.as_str()
                );
                failures.push(app_type.as_str().to_string());
                return;
            }
        };
        if !is_current && !crate::mode::current::is_proxy(&app_type) {
            return;
        }

        // The caller owns the application lock from the old-row read through
        // publication. Also reproject non-default Stack members in proxy mode.
        let written = state
            .db
            .get_provider_by_id(child_id, &app_type)
            .and_then(|provider| {
                let provider = provider.ok_or_else(|| {
                    AppError::InvalidInput(format!("Provider not found: {child_id}"))
                })?;
                live::sync_live_for_provider_respecting_mode(state, &app_type, &provider, None)
                    .map(|_| ())
            });
        if let Err(err) = written {
            log::warn!(
                "统一供应商同步后重写 {} live 配置失败: {err}",
                app_type.as_str()
            );
            failures.push(app_type.as_str().to_string());
        }
    }

    /// 递归合并 JSON：base 为底，patch 覆盖同名字段
    fn merge_json(base: &mut serde_json::Value, patch: &serde_json::Value) {
        use serde_json::Value;

        match (base, patch) {
            (Value::Object(base_map), Value::Object(patch_map)) => {
                for (k, v_patch) in patch_map {
                    match base_map.get_mut(k) {
                        Some(v_base) => Self::merge_json(v_base, v_patch),
                        None => {
                            base_map.insert(k.clone(), v_patch.clone());
                        }
                    }
                }
            }
            // 其它类型：直接覆盖
            (base_val, patch_val) => {
                *base_val = patch_val.clone();
            }
        }
    }
}
