#![allow(non_snake_case)]

use indexmap::IndexMap;
use std::collections::HashMap;

use serde::Serialize;
use tauri::State;

use super::run_db_task;
use crate::app_config::AppType;
use crate::claude_mcp;
use crate::services::McpService;
use crate::store::AppState;

async fn run_mcp_mutation<T, F>(state: &State<'_, AppState>, task: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce(&AppState) -> Result<T, String> + Send + 'static,
{
    let guards = McpService::lock_live_sync_apps(state.inner()).await;
    run_db_task(state, move |state| {
        let _guards = guards;
        task(state)
    })
    .await
}

/// Start a bounded, one-shot batch only while the MCP page is mounted.
#[tauri::command]
pub async fn start_mcp_connectivity_checks(
    state: State<'_, AppState>,
    server_ids: Vec<String>,
    on_result: tauri::ipc::Channel<crate::services::mcp_connectivity::ConnectivityResult>,
) -> Result<String, String> {
    let servers = run_db_task(&state, |state| {
        McpService::get_all_servers(state).map_err(|e| e.to_string())
    })
    .await?;
    Ok(crate::services::mcp_connectivity::start_checks(
        // Use the page's enabled selection: app toggles update it optimistically,
        // so the database's enablement flags can still lag behind the UI.
        servers
            .into_iter()
            .filter(|(id, _)| server_ids.contains(id))
            .map(|(_, server)| server)
            .collect(),
        move |result| on_result.send(result).is_ok(),
    ))
}

#[tauri::command]
pub fn cancel_mcp_connectivity_checks(batch_id: String) {
    crate::services::mcp_connectivity::cancel_checks(&batch_id);
}

#[tauri::command]
pub async fn get_claude_mcp_status() -> Result<claude_mcp::McpStatus, String> {
    claude_mcp::get_mcp_status().map_err(|e| e.to_string())
}

/// 读取 mcp.json 文本内容
#[tauri::command]
pub async fn read_claude_mcp_config() -> Result<Option<String>, String> {
    claude_mcp::read_mcp_json().map_err(|e| e.to_string())
}

/// 新增或更新一个 MCP 服务器条目
#[tauri::command]
pub async fn upsert_claude_mcp_server(
    state: State<'_, AppState>,
    id: String,
    spec: serde_json::Value,
) -> Result<bool, String> {
    let guard = state.proxy_service.lock_switch_for_app("claude").await;
    run_db_task(&state, move |_| {
        let _guard = guard;
        claude_mcp::upsert_mcp_server(&id, spec).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_claude_mcp_server(
    state: State<'_, AppState>,
    id: String,
) -> Result<bool, String> {
    let guard = state.proxy_service.lock_switch_for_app("claude").await;
    run_db_task(&state, move |_| {
        let _guard = guard;
        claude_mcp::delete_mcp_server(&id).map_err(|e| e.to_string())
    })
    .await
}

/// 校验命令是否在 PATH 中可用（不执行）
#[tauri::command]
pub async fn validate_mcp_command(cmd: String) -> Result<bool, String> {
    claude_mcp::validate_command_in_path(&cmd).map_err(|e| e.to_string())
}

#[derive(Serialize)]
pub struct McpConfigResponse {
    pub config_path: String,
    pub servers: HashMap<String, serde_json::Value>,
}

/// 获取 MCP 配置（来自 ~/.cc-switch/config.json）
use std::str::FromStr;

#[tauri::command]
#[allow(deprecated)] // 兼容层命令，内部调用已废弃的 Service 方法
pub async fn get_mcp_config(
    state: State<'_, AppState>,
    app: String,
) -> Result<McpConfigResponse, String> {
    let config_path = crate::config::get_app_config_path()
        .to_string_lossy()
        .to_string();
    let app_ty = AppType::from_str(&app).map_err(|e| e.to_string())?;
    let servers = McpService::get_servers(&state, app_ty).map_err(|e| e.to_string())?;
    Ok(McpConfigResponse {
        config_path,
        servers,
    })
}

/// 在 config.json 中新增或更新一个 MCP 服务器定义
/// [已废弃] 该命令仍然使用旧的分应用API，会转换为统一结构
#[tauri::command]
pub async fn upsert_mcp_server_in_config(
    state: State<'_, AppState>,
    app: String,
    id: String,
    spec: serde_json::Value,
    sync_other_side: Option<bool>,
) -> Result<bool, String> {
    run_mcp_mutation(&state, move |state| {
        upsert_legacy_mcp_server(state, app, id, spec, sync_other_side)
    })
    .await
}

fn upsert_legacy_mcp_server(
    state: &AppState,
    app: String,
    id: String,
    spec: serde_json::Value,
    sync_other_side: Option<bool>,
) -> Result<bool, String> {
    use crate::app_config::McpServer;

    let app_ty = AppType::from_str(&app).map_err(|e| e.to_string())?;

    // 读取现有的服务器（如果存在）
    let existing_server = {
        let servers = state.db.get_all_mcp_servers().map_err(|e| e.to_string())?;
        servers.get(&id).cloned()
    };

    // 构建新的统一服务器结构
    let mut new_server = if let Some(mut existing) = existing_server {
        // 更新现有服务器
        existing.server = spec.clone();
        existing.apps.set_enabled_for(&app_ty, true);
        existing
    } else {
        // 创建新服务器
        let mut apps = crate::app_config::McpApps::default();
        apps.set_enabled_for(&app_ty, true);

        // 尝试从 spec 中提取 name，否则使用 id
        let name = spec
            .get("name")
            .and_then(|v| v.as_str())
            .unwrap_or(&id)
            .to_string();

        McpServer {
            id: id.clone(),
            name,
            server: spec,
            apps,
            description: None,
            homepage: None,
            docs: None,
            tags: Vec::new(),
        }
    };

    // 如果 sync_other_side 为 true，也启用其他应用
    if sync_other_side.unwrap_or(false) {
        new_server.apps.claude = true;
        new_server.apps.codex = true;
        new_server.apps.gemini = true;
        new_server.apps.opencode = true;
    }

    McpService::upsert_server(state, new_server)
        .map(|_| true)
        .map_err(|e| e.to_string())
}

/// 在 config.json 中删除一个 MCP 服务器定义
#[tauri::command]
pub async fn delete_mcp_server_in_config(
    state: State<'_, AppState>,
    _app: String, // 参数保留用于向后兼容，但在统一结构中不再需要
    id: String,
) -> Result<bool, String> {
    run_mcp_mutation(&state, move |state| {
        McpService::delete_server(state, &id).map_err(|e| e.to_string())
    })
    .await
}

/// 设置启用状态并同步到客户端配置（兼容层命令，同 `toggle_mcp_app`）
#[tauri::command]
pub async fn set_mcp_enabled(
    state: State<'_, AppState>,
    app: String,
    id: String,
    enabled: bool,
) -> Result<bool, String> {
    let app_ty = AppType::from_str(&app).map_err(|e| e.to_string())?;
    McpService::toggle_app_locked(&state, &id, app_ty, enabled)
        .await
        .map(|_| true)
        .map_err(|e| e.to_string())
}

// ============================================================================
// v3.7.0 新增：统一 MCP 管理命令
// ============================================================================

use crate::app_config::McpServer;

/// 获取所有 MCP 服务器（统一结构）
#[tauri::command]
pub async fn get_mcp_servers(
    state: State<'_, AppState>,
) -> Result<IndexMap<String, McpServer>, String> {
    McpService::get_all_servers(&state).map_err(|e| e.to_string())
}

/// 库里读不出来的 MCP 服务器（只给界面展示、删除）。
#[tauri::command]
pub async fn get_unreadable_mcp_servers(
    state: State<'_, AppState>,
) -> Result<Vec<crate::database::UnreadableEntry>, String> {
    McpService::list_unreadable(&state).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn upsert_mcp_server(
    state: State<'_, AppState>,
    server: McpServer,
) -> Result<(), String> {
    // 与 toggle/resync 共用逐应用切换锁，避免整文件读改写互相覆盖。
    run_mcp_mutation(&state, move |state| {
        McpService::upsert_server(state, server).map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn delete_mcp_server(state: State<'_, AppState>, id: String) -> Result<bool, String> {
    run_mcp_mutation(&state, move |state| {
        McpService::delete_server(state, &id).map_err(|e| e.to_string())
    })
    .await
}

/// 切换 MCP 服务器在指定应用的启用状态
#[tauri::command]
pub async fn toggle_mcp_app(
    state: State<'_, AppState>,
    server_id: String,
    app: String,
    enabled: bool,
) -> Result<(), String> {
    let app_ty = AppType::from_str(&app).map_err(|e| e.to_string())?;
    McpService::toggle_app_locked(&state, &server_id, app_ty, enabled)
        .await
        .map_err(|e| e.to_string())
}

/// 从所有应用导入 MCP 服务器（复用已有的导入逻辑）
#[tauri::command]
pub async fn import_mcp_from_apps(state: State<'_, AppState>) -> Result<usize, String> {
    run_mcp_mutation(&state, |state| {
        McpService::import_from_all_apps(state).map_err(|e| e.to_string())
    })
    .await
}

/// 按数据库里的开关把 MCP 重新写进各应用的 live 配置，逐应用返回结果。
///
/// `apps` 缺省或为空时同步全部受管应用。每个应用先拿它的切换锁再写，
/// 和切换供应商互斥；单个应用失败（如配置文件解析失败）不影响其余应用。
#[tauri::command]
pub async fn resync_mcp_to_apps(
    state: State<'_, AppState>,
    apps: Option<Vec<String>>,
) -> Result<Vec<crate::services::mcp::McpAppSyncOutcome>, String> {
    let targets = McpService::resync_targets(apps.as_deref()).map_err(|e| e.to_string())?;
    let mut outcomes = Vec::with_capacity(targets.len());
    for app in targets {
        let _guard = state.proxy_service.lock_switch_for_app(app.as_str()).await;
        outcomes.push(McpService::resync_app(&state, &app));
    }
    Ok(outcomes)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::future::Future;
    use std::sync::Arc;
    use std::task::Context;
    use std::time::Duration;
    use tauri::Manager;

    #[test]
    #[serial_test::serial]
    fn legacy_mcp_upsert_waits_before_reading_existing_server() {
        let _home = crate::commands::startup_tests::TestHome::new();
        let db = Arc::new(crate::database::Database::memory().unwrap());
        let app = tauri::test::mock_builder()
            .manage(AppState::new(db.clone()))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let mut server = McpServer {
            id: "fixture".into(),
            name: "original".into(),
            server: serde_json::json!({"command": "echo"}),
            apps: Default::default(),
            description: None,
            homepage: None,
            docs: None,
            tags: vec![],
        };
        db.save_mcp_server(&server).unwrap();
        let state = app.state::<AppState>();
        let guard =
            tauri::async_runtime::block_on(state.proxy_service.lock_switch_for_app("codex"));
        let mut update = std::pin::pin!(upsert_mcp_server_in_config(
            app.state(),
            "codex".into(),
            "fixture".into(),
            serde_json::json!({"command": "echo", "args": ["new"]}),
            None,
        ));
        let first_poll = update
            .as_mut()
            .poll(&mut Context::from_waker(futures::task::noop_waker_ref()));
        server.name = "concurrent edit".into();
        server.apps.gemini = true;
        db.save_mcp_server(&server).unwrap();
        drop(guard);
        assert!(first_poll.is_pending());
        tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_secs(5), update)
                .await
                .unwrap()
                .unwrap();
        });
        let saved = db.get_all_mcp_servers().unwrap()["fixture"].clone();
        assert_eq!(saved.name, "concurrent edit");
        assert!(saved.apps.gemini && saved.apps.codex);
        assert_eq!(saved.server["args"][0], "new");
    }

    #[test]
    #[serial_test::serial]
    fn legacy_mcp_delete_waits_for_switch_lock() {
        let _home = crate::commands::startup_tests::TestHome::new();
        let db = Arc::new(crate::database::Database::memory().unwrap());
        let app = tauri::test::mock_builder()
            .manage(AppState::new(db))
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        let state = app.state::<AppState>();
        let guard =
            tauri::async_runtime::block_on(state.proxy_service.lock_switch_for_app("codex"));
        let mut deletion = std::pin::pin!(delete_mcp_server_in_config(
            app.state(),
            "codex".into(),
            "missing".into(),
        ));
        let first_poll = deletion
            .as_mut()
            .poll(&mut Context::from_waker(futures::task::noop_waker_ref()));
        drop(guard);
        assert!(first_poll.is_pending());
        let deleted = tauri::async_runtime::block_on(async {
            tokio::time::timeout(Duration::from_secs(5), deletion)
                .await
                .unwrap()
                .unwrap()
        });
        assert!(!deleted);
    }
}
