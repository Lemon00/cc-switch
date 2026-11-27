use indexmap::IndexMap;
use std::collections::HashMap;

use crate::app_config::{AppType, McpServer, MultiAppConfig};
use crate::database::UnreadableEntry;
use crate::error::AppError;
use crate::mcp;
use crate::store::AppState;

/// MCP 相关业务逻辑（v3.7.0 统一结构）
pub struct McpService;

/// 「重新同步到各应用」里单个应用的结果
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct McpAppSyncOutcome {
    pub app: String,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

mod commit;
use commit::{commits_with_db, McpWritePlan};

impl McpService {
    /// 获取所有 MCP 服务器（统一结构）
    /// 读不出来的行不在这里，见 [`Self::list_unreadable`]。
    pub fn get_all_servers(state: &AppState) -> Result<IndexMap<String, McpServer>, AppError> {
        Ok(state.db.get_mcp_server_rows()?.readable)
    }

    /// 库里读不出来的 MCP 服务器：界面单独列出，只能删除。
    pub fn list_unreadable(state: &AppState) -> Result<Vec<UnreadableEntry>, AppError> {
        Ok(state.db.get_mcp_server_rows()?.unreadable)
    }

    /// 命令入口：先按涉及的应用拿切换锁再 upsert，避免与 toggle/resync 交错丢更新。
    pub async fn upsert_server_locked(state: &AppState, server: McpServer) -> Result<(), AppError> {
        let _guards = Self::lock_live_sync_apps(state).await;
        Self::upsert_server(state, server)
    }

    pub async fn lock_live_sync_apps(state: &AppState) -> Vec<tokio::sync::OwnedMutexGuard<()>> {
        // Shared records can still own retired deployments. Match McpWritePlan's full
        // write set, including cleanup, rather than the product's visible app list.
        let mut apps: Vec<_> = AppType::persisted().filter(AppType::supports_mcp).collect();
        apps.sort_by_key(|app| app.as_str().to_string());
        apps.dedup();
        let mut guards = Vec::with_capacity(apps.len());
        for app in &apps {
            guards.push(state.operations.lock_for_app(app.as_str()).await);
        }
        guards
    }

    /// 添加或更新 MCP 服务器
    pub fn upsert_server(state: &AppState, server: McpServer) -> Result<(), AppError> {
        if server.id.trim().is_empty() || server.id.chars().any(char::is_control) {
            return Err(AppError::McpValidation(
                "MCP 服务器 ID 不能为空或包含控制字符".into(),
            ));
        }
        mcp::validation::validate_server_spec(&server.server)?;
        // 读取旧状态：用于处理“编辑时取消勾选某个应用”的场景（需要从对应 live 配置中移除）。
        // 别的服务器读不出来不影响；这个 id 本身读不出来就在写任何 live 之前拒绝。
        let previous = state.db.get_mcp_server_rows()?.get(&server.id)?.cloned();
        McpWritePlan::upsert(&server, previous.as_ref())
            .execute(state, || state.db.save_mcp_server(&server))
    }

    fn sync_legacy_and_commit<T>(
        server: &McpServer,
        app: &AppType,
        enabled: bool,
        commit: impl FnOnce() -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let path = match app {
            AppType::Claude => crate::config::get_claude_mcp_path(),
            AppType::Codex => crate::codex_config::get_codex_config_path(),

            AppType::OpenCode => crate::opencode_config::get_opencode_config_path()?,

            _ => return commit(),
        };
        let result = crate::sync_utils::write_and_commit(
            &path,
            || {
                if enabled {
                    Self::sync_server_to_app(server, app)
                } else {
                    Self::remove_server_from_app(&server.id, app)
                }
            },
            commit,
        );
        if result.is_err() && app == &AppType::Claude {
            crate::services::claude_account::sync_all_shared(&crate::config::get_app_config_dir());
        }
        result
    }

    /// 命令入口：先按涉及应用拿切换锁再删除。
    pub async fn delete_server_locked(state: &AppState, id: &str) -> Result<bool, AppError> {
        let _guards = Self::lock_live_sync_apps(state).await;
        Self::delete_server(state, id)
    }

    /// 删除 MCP 服务器
    pub fn delete_server(state: &AppState, id: &str) -> Result<bool, AppError> {
        // 读不出来的行只删库里这一行：不知道它往各应用写过什么，不碰 live，也不写回任何东西。
        if state.db.get_unreadable_mcp_server(id)?.is_some() {
            state.db.delete_mcp_server(id)?;
            return Ok(true);
        }
        let server = state.db.get_mcp_server_rows()?.readable.shift_remove(id);

        if let Some(server) = server {
            McpWritePlan::delete(&server).execute(state, || state.db.delete_mcp_server(id))?;
            Ok(true)
        } else {
            Ok(false)
        }
    }

    /// 命令入口用的 [`Self::toggle_app`]：先拿这个应用的切换锁再读改写 live 配置。
    ///
    /// 逐条写入是「读文件 → 改 → 写回」，并发的两次切换（批量勾选、和切换供应商或
    /// 重新同步撞在一起）会互相覆盖、丢掉对方写的条目。和 `resync_mcp_to_apps` 用同一把
    /// 锁。`toggle_app` 内部不拿这把锁（tokio 的锁不可重入），已经持锁的调用方直接调它。
    pub async fn toggle_app_locked(
        state: &AppState,
        server_id: &str,
        app: AppType,
        enabled: bool,
    ) -> Result<(), AppError> {
        let _guard = state.operations.lock_for_app(app.as_str()).await;
        Self::toggle_app(state, server_id, app, enabled)
    }

    /// 切换指定应用的启用状态。调用方负责先拿这个应用的切换锁（见 [`Self::toggle_app_locked`]）。
    pub fn toggle_app(
        state: &AppState,
        server_id: &str,
        app: AppType,
        enabled: bool,
    ) -> Result<(), AppError> {
        if !app.supports_mcp() {
            return Err(AppError::McpValidation(format!(
                "{} 不支持由 CC Switch 管理 MCP",
                app.as_str()
            )));
        }
        let rows = state.db.get_mcp_server_rows()?;
        let Some(server) = rows.get(server_id)? else {
            return Err(AppError::Message(format!(
                "MCP 服务器 '{server_id}' 不存在"
            )));
        };
        McpWritePlan::toggle(server, app.clone(), enabled).execute(state, || {
            state
                .db
                .update_mcp_server_app_enabled(server_id, &app, enabled)
                .map(|_| ())
        })
    }

    /// 将 MCP 服务器同步到指定应用
    fn sync_server_to_app(server: &McpServer, app: &AppType) -> Result<(), AppError> {
        mcp::validation::validate_server_spec(&server.server)?;
        match app {
            AppType::Claude => {
                mcp::sync_single_server_to_claude(&server.id, &server.server)?;
                // 各官方账号目录镜像默认 .claude.json 的 mcpServers；失败要向上返回，
                // 否则 write_and_commit 会提交 DB，账号目录却还是旧 MCP。
                crate::services::claude_account::sync_all_shared_checked(
                    &crate::config::get_app_config_dir(),
                )?;
            }

            AppType::Codex => {
                // Codex uses TOML format, must use the correct function
                mcp::sync_single_server_to_codex(&server.id, &server.server)?;
            }

            AppType::OpenCode => {
                mcp::sync_single_server_to_opencode(&server.id, &server.server)?;
            }

            AppType::Dsh => mcp::dsh::sync(&server.id, Some(&server.server))?,

            AppType::Agy => mcp::agy::sync(&server.id, Some(&server.server))?,
        }
        Ok(())
    }

    fn remove_server_from_app(id: &str, app: &AppType) -> Result<(), AppError> {
        match app {
            AppType::Claude => {
                mcp::remove_server_from_claude(id)?;
                crate::services::claude_account::sync_all_shared_checked(
                    &crate::config::get_app_config_dir(),
                )?;
            }

            AppType::Codex => mcp::remove_server_from_codex(id)?,

            AppType::OpenCode => {
                mcp::remove_server_from_opencode(id)?;
            }

            AppType::Dsh => mcp::dsh::sync(id, None)?,

            AppType::Agy => mcp::agy::sync(id, None)?,
        }
        Ok(())
    }

    /// 手动同步所有启用的 MCP 服务器到对应的应用。
    ///
    /// Best-effort：单个应用投影失败（如 ~/.claude.json 坏 JSON）不阻断
    /// 其余应用——各应用的 live 文件互相独立，一处损坏没有理由让其他
    /// 应用的 MCP 状态陈旧。全部跑完后若有失败，聚合成一个错误上报，
    /// 保留调用方的可见性。
    pub fn sync_all_enabled(state: &AppState) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;

        let mut failures: Vec<String> = Vec::new();
        for app in AppType::all() {
            if let Err(err) = Self::project_servers_to_app(&servers, &app) {
                log::warn!("同步 MCP 到 {app:?} 失败: {err}");
                failures.push(format!("{}: {err}", app.as_str()));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::Message(format!(
                "部分应用 MCP 同步失败: {}",
                failures.join("; ")
            )))
        }
    }

    /// 只把启用状态投影到单个应用。某个应用的 live 被整体重写后用它做
    /// 定向重投影，避免把无关应用的失败面（如 ~/.claude.json 坏 JSON）
    /// 牵连进目标应用的关键路径。
    pub fn sync_enabled_for_app(state: &AppState, app: &AppType) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;
        Self::project_servers_to_app(&servers, app)
    }

    /// 由 CC Switch 管理 MCP 的应用（Claude Desktop、OpenClaw 不支持）
    pub fn live_sync_apps() -> Vec<AppType> {
        AppType::all().filter(AppType::supports_mcp).collect()
    }

    /// 解析「重新同步」的目标应用：缺省或空列表＝全部受管应用；
    /// 不认识或不支持 MCP 的应用直接报错，不静默跳过。
    pub fn resync_targets(apps: Option<&[String]>) -> Result<Vec<AppType>, AppError> {
        let managed = Self::live_sync_apps();
        let Some(apps) = apps.filter(|apps| !apps.is_empty()) else {
            return Ok(managed);
        };
        let mut targets = Vec::new();
        for raw in apps {
            let app = <AppType as std::str::FromStr>::from_str(raw)?;
            if !managed.contains(&app) {
                return Err(AppError::Message(format!(
                    "{} 不支持由 CC Switch 管理 MCP",
                    app.as_str()
                )));
            }
            if !targets.contains(&app) {
                targets.push(app);
            }
        }
        Ok(targets)
    }

    /// 把数据库里的启用状态重新投影到一个应用的 live 配置，结果按应用报告。
    /// 调用方负责先拿这个应用的切换锁。
    pub fn resync_app(state: &AppState, app: &AppType) -> McpAppSyncOutcome {
        match Self::sync_enabled_for_app(state, app) {
            Ok(()) => McpAppSyncOutcome {
                app: app.as_str().to_string(),
                ok: true,
                error: None,
            },
            Err(err) => {
                log::warn!("重新同步 MCP 到 {app:?} 失败: {err}");
                McpAppSyncOutcome {
                    app: app.as_str().to_string(),
                    ok: false,
                    error: Some(err.to_string()),
                }
            }
        }
    }

    fn project_servers_to_app(
        servers: &IndexMap<String, McpServer>,
        app: &AppType,
    ) -> Result<(), AppError> {
        // 逐条容错：一条写不进去（规范不是对象、这个应用不认的类型）不该让排在
        // 它后面的服务器全都停在旧状态。全部跑完后聚合上报，调用方照旧能看到
        // 失败。错误相同的条目归并成一项：live 文件本身损坏时每一条都报同一句。
        let mut failures: IndexMap<String, Vec<&str>> = IndexMap::new();
        for server in servers.values() {
            let result = if server.apps.is_enabled_for(app) {
                Self::sync_server_to_app(server, app)
            } else if !commits_with_db(app) {
                Self::remove_server_from_app(&server.id, app)
            } else {
                // MCode / Pi's false flag also covers pre-existing, unmanaged servers.
                // Only explicit disable/delete operations may remove those entries.
                Ok(())
            };
            if let Err(err) = result {
                log::warn!("同步 MCP 服务器 '{}' 到 {app:?} 失败: {err}", server.id);
                failures
                    .entry(err.to_string())
                    .or_default()
                    .push(&server.id);
            }
        }

        if failures.is_empty() {
            return Ok(());
        }
        let detail = failures
            .iter()
            .map(|(error, ids)| format!("{}: {error}", ids.join(", ")))
            .collect::<Vec<_>>()
            .join("; ");
        Err(AppError::Message(format!(
            "部分 MCP 服务器同步到 {} 失败: {detail}",
            app.as_str()
        )))
    }

    // ========================================================================
    // 兼容层：支持旧的 v3.6.x 命令（已废弃，将在 v4.0 移除）
    // ========================================================================

    /// [已废弃] 获取指定应用的 MCP 服务器（兼容旧 API）
    #[deprecated(since = "3.7.0", note = "Use get_all_servers instead")]
    pub fn get_servers(
        state: &AppState,
        app: AppType,
    ) -> Result<HashMap<String, serde_json::Value>, AppError> {
        let all_servers = Self::get_all_servers(state)?;
        let mut result = HashMap::new();

        for (id, server) in all_servers {
            if server.apps.is_enabled_for(&app) {
                result.insert(id, server.server);
            }
        }

        Ok(result)
    }

    /// [已废弃] 设置 MCP 服务器在指定应用的启用状态（兼容旧 API）
    #[deprecated(since = "3.7.0", note = "Use toggle_app instead")]
    pub fn set_enabled(
        state: &AppState,
        app: AppType,
        id: &str,
        enabled: bool,
    ) -> Result<bool, AppError> {
        Self::toggle_app(state, id, app, enabled)?;
        Ok(true)
    }

    /// [已废弃] 同步启用的 MCP 到指定应用（兼容旧 API）
    #[deprecated(since = "3.7.0", note = "Use sync_all_enabled instead")]
    pub fn sync_enabled(state: &AppState, app: AppType) -> Result<(), AppError> {
        let servers = Self::get_all_servers(state)?;

        for server in servers.values() {
            if server.apps.is_enabled_for(&app) {
                Self::sync_server_to_app(server, &app)?;
            }
        }

        Ok(())
    }

    /// 从单个应用的 live 配置导入 MCP，返回新导入的数量。
    ///
    /// 已存在的服务器只启用该应用，不覆盖其他字段。导入只读 live 配置，
    /// 不反向写回任何应用；显式编辑、启用/禁用或手动同步时再写回。
    pub fn import_from_app(state: &AppState, app: &AppType) -> Result<usize, AppError> {
        let read_live: fn(&mut MultiAppConfig) -> Result<usize, AppError> = match app {
            AppType::Claude => mcp::import_from_claude,
            AppType::Codex => mcp::import_from_codex,

            AppType::OpenCode => mcp::import_from_opencode,

            AppType::Dsh => return mcp::dsh::import(state),
            AppType::Agy => return mcp::agy::import(state),
        };

        let mut temp_config = MultiAppConfig::default();
        if read_live(&mut temp_config)? == 0 {
            return Ok(0);
        }
        let Some(servers) = &temp_config.mcp.servers else {
            return Ok(0);
        };
        let (mut existing, unreadable) = state
            .db
            .get_mcp_server_rows()?
            .into_readable_and_unreadable_ids();
        let mut new_count = 0;
        let mut skipped = Vec::new();
        for server in servers.values() {
            if unreadable.contains(&server.id) {
                skipped.push(format!(
                    "'{}': existing database row is unreadable",
                    server.id
                ));
                continue;
            }
            let to_save = if let Some(existing_server) = existing.get(&server.id) {
                if mcp::validation::transport_spec(&existing_server.server)
                    != mcp::validation::transport_spec(&server.server)
                {
                    skipped.push(format!(
                        "'{}': conflicts with an existing server",
                        server.id
                    ));
                    continue;
                }
                let mut merged = existing_server.clone();
                merged
                    .apps
                    .set_enabled_for(app, server.apps.is_enabled_for(app));
                merged
            } else {
                new_count += 1;
                server.clone()
            };
            state.db.save_mcp_server(&to_save)?;
            existing.insert(to_save.id.clone(), to_save);
        }
        mcp::import_outcome(new_count, app.as_str(), skipped)
    }

    /// 从所有支持 MCP 的应用导入服务器，返回新导入的数量。
    ///
    /// Best-effort：单个应用导入失败（如坏 config.toml）不阻断其余应用；
    /// 全部跑完后若有失败，聚合成一个错误上报——历史实现逐应用
    /// `unwrap_or(0)` 吞错，坏文件只会表现为"导入成功 0 个"，用户
    /// 无从得知哪个应用出了问题。
    pub fn import_from_all_apps(state: &AppState) -> Result<usize, AppError> {
        let mut total = 0;
        let mut failures: Vec<String> = Vec::new();

        for app in Self::live_sync_apps() {
            match Self::import_from_app(state, &app) {
                Ok(count) => total += count,
                Err(err) => {
                    log::warn!("从 {} 导入 MCP 失败: {err}", app.as_str());
                    failures.push(format!("{}: {err}", app.as_str()));
                }
            }
        }

        if failures.is_empty() {
            Ok(total)
        } else {
            Err(AppError::Message(format!(
                "已导入 {total} 个，部分应用导入失败: {}",
                failures.join("; ")
            )))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        database::Database,
        settings::{self, AppSettings},
    };
    use std::sync::Arc;

    struct TestSettings {
        directory: tempfile::TempDir,
        previous_home: Option<std::ffi::OsString>,
        previous: AppSettings,
    }

    impl TestSettings {
        fn new() -> Self {
            let directory = tempfile::tempdir().unwrap();
            let previous_home = std::env::var_os("CC_SWITCH_TEST_HOME");
            std::env::set_var("CC_SWITCH_TEST_HOME", directory.path());
            let previous = settings::get_settings();
            let guard = Self {
                directory,
                previous_home,
                previous,
            };
            assert!(crate::config::get_app_config_dir().starts_with(guard.directory.path()));
            let codex = guard.directory.path().join("codex");
            let gemini = guard.directory.path().join("gemini");
            std::fs::create_dir_all(&codex).unwrap();
            std::fs::create_dir_all(&gemini).unwrap();
            settings::update_settings(AppSettings {
                codex_config_dir: Some(codex.to_string_lossy().into_owned()),
                gemini_config_dir: Some(gemini.to_string_lossy().into_owned()),
                ..AppSettings::default()
            })
            .unwrap();
            guard
        }
    }

    impl Drop for TestSettings {
        fn drop(&mut self) {
            if crate::config::get_app_config_dir().starts_with(self.directory.path()) {
                let _ = settings::update_settings(self.previous.clone());
            }
            match &self.previous_home {
                Some(previous) => std::env::set_var("CC_SWITCH_TEST_HOME", previous),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
        }
    }

    fn server(enabled: bool) -> McpServer {
        serde_json::from_value(serde_json::json!({
            "id": "example", "name": "example",
            "server": {"command": "echo", "args": ["new"]},
            "apps": {"codex": enabled}
        }))
        .unwrap()
    }

    #[test]
    #[serial_test::serial]
    fn importing_disabled_legacy_servers_preserves_the_native_flag() {
        let _settings = TestSettings::new();
        let path = crate::codex_config::get_codex_config_path();
        let original = "[mcp_servers.example]\ncommand = 'echo'\nenabled = false\n";
        std::fs::write(&path, original).unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        for existing in [false, true] {
            if existing {
                let mut previous = server(true);
                previous.server = serde_json::json!({"command":"echo"});
                state.db.save_mcp_server(&previous).unwrap();
            }
            McpService::import_from_app(&state, &AppType::Codex).unwrap();
            let saved = state.db.get_all_mcp_servers().unwrap();
            assert!(!saved["example"].apps.codex);
            assert!(saved["example"].server.get("enabled").is_none());
            assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
        }
    }

    #[test]
    #[serial_test::serial]
    fn legacy_import_reports_same_name_connection_conflicts_without_claiming_the_app() {
        let _settings = TestSettings::new();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        state.db.save_mcp_server(&server(false)).unwrap();
        let path = crate::codex_config::get_codex_config_path();
        let original =
            "[mcp_servers.example]\ncommand = 'different'\n[mcp_servers.valid]\ncommand = 'echo'\n";
        std::fs::write(&path, original).unwrap();
        let error = McpService::import_from_app(&state, &AppType::Codex)
            .unwrap_err()
            .to_string();
        assert!(error.contains("example"));
        let saved = state.db.get_all_mcp_servers().unwrap();
        assert!(!saved["example"].apps.codex);
        assert_eq!(saved["example"].server, server(false).server);
        assert!(saved["valid"].apps.codex);
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    #[serial_test::serial]
    fn malformed_native_server_maps_fail_without_changing_files_or_database() {
        let _settings = TestSettings::new();
        for (app, path) in [
            (AppType::Claude, crate::config::get_claude_mcp_path()),
            (
                AppType::Agy,
                crate::agy_config::customizations_dir().join("mcp_config.json"),
            ),
        ] {
            for original in ["{\"mcpServers\":[]}", "{\"mcpServers\":null}", "[]"] {
                std::fs::create_dir_all(path.parent().unwrap()).unwrap();
                std::fs::write(&path, original).unwrap();
                let state = AppState::new(Arc::new(Database::memory().unwrap()));
                let mut requested = server(false);
                requested.apps.set_enabled_for(&app, true);
                assert!(McpService::upsert_server(&state, requested).is_err());
                assert!(state.db.get_all_mcp_servers().unwrap().is_empty());
                assert_eq!(std::fs::read_to_string(&path).unwrap(), original);
            }
        }
    }

    #[test]
    #[serial_test::serial]
    fn repeated_toggle_and_delete_restore_exact_bytes_when_database_rejects() {
        let _settings = TestSettings::new();
        let path = crate::codex_config::get_codex_config_path();
        let original = b"# keep this comment\nmodel = 'keep'\n[mcp_servers.example]\ncommand = 'echo'\nargs = ['old']\n";
        for enabled in [true, false] {
            let state = AppState::new(Arc::new(Database::memory().unwrap()));
            state.db.save_mcp_server(&server(enabled)).unwrap();
            std::fs::write(&path, original).unwrap();
            state
                .db
                .conn
                .lock()
                .unwrap()
                .execute_batch("PRAGMA query_only = ON")
                .unwrap();
            assert!(McpService::toggle_app(&state, "example", AppType::Codex, enabled).is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert_eq!(
                state.db.get_all_mcp_servers().unwrap()["example"]
                    .apps
                    .codex,
                enabled
            );
            assert!(McpService::delete_server(&state, "example").is_err());
            assert_eq!(std::fs::read(&path).unwrap(), original);
            assert!(state
                .db
                .get_all_mcp_servers()
                .unwrap()
                .contains_key("example"));
        }
    }

    #[test]
    #[serial_test::serial]
    fn later_application_failure_rolls_back_earlier_files_and_skips_database_commit() {
        let _settings = TestSettings::new();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        let codex_path = crate::codex_config::get_codex_config_path();
        std::fs::write(&codex_path, "invalid toml [").unwrap();
        // Claude is written before Codex in the plan. A native-first adapter would
        // fail before either of them, so would not exercise the compensating write.
        let claude_path = crate::config::get_claude_mcp_path();
        let original = b"{\n  \"mcpServers\": {}, \"keep\": true\n}\n";
        std::fs::create_dir_all(claude_path.parent().unwrap()).unwrap();
        std::fs::write(&claude_path, original).unwrap();
        let mut requested = server(true);
        requested.apps.claude = true;
        assert!(McpService::upsert_server(&state, requested).is_err());
        assert_eq!(std::fs::read(&claude_path).unwrap(), original);
        assert_eq!(
            std::fs::read_to_string(&codex_path).unwrap(),
            "invalid toml ["
        );
        assert!(state.db.get_all_mcp_servers().unwrap().is_empty());
    }
}
