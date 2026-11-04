//! Codex OAuth 账号登出/删除时要与「供应商切换」串行化。
//!
//! 这两个函数原本在 GUI 的 `commands::auth` 里，但实现全是核心能力
//! （`ProxyService::lock_switch_for_app` + `CodexOAuthManager` + `db`），
//! core 的测试（`services/provider/mod.rs` 的绑定解绑用例）也在用它们。
//! 放在这里 GUI / CLI / TUI 都能调，GUI 的命令层只留一层转发。

use crate::app_config::AppType;
use crate::store::AppState;

/// 登出本机全部 Codex 托管账号，并把绑定它们的供应商解绑。
///
/// 只处理本机账号：从别的设备同步过来、绑定在对方设备上的行保持不动。
pub async fn logout_codex_oauth_with_switch_lock(state: &AppState) -> Result<(), String> {
    let _switch_guard = state
        .proxy_service
        .lock_switch_for_app(AppType::Codex.as_str())
        .await;
    // Only this device's accounts: rows synced from another device stay bound there.
    let removed: Vec<String> = state
        .codex_oauth_manager
        .list_accounts()
        .await
        .into_iter()
        .map(|account| account.id)
        .collect();
    state
        .codex_oauth_manager
        .clear_auth()
        .await
        .map_err(|error| error.to_string())?;
    state
        .db
        .unbind_codex_managed_accounts(&removed)
        .map_err(|error| {
            format!("托管账号已登出，但 Codex 供应商解绑失败；请编辑供应商重新选择账号或切换供应商: {error}")
        })?;
    Ok(())
}

/// 删除单个 Codex 托管账号并解绑引用它的供应商。
///
/// 与切换串行化的原因：一次已经预检过 bundle 的切换会把 auth.json 重新写出来，
/// 不串行的话删掉的凭据会复活。
pub async fn remove_codex_oauth_account_with_switch_lock(
    state: &AppState,
    account_id: &str,
) -> Result<(), String> {
    let _switch_guard = state
        .proxy_service
        .lock_switch_for_app(AppType::Codex.as_str())
        .await;
    state
        .codex_oauth_manager
        .remove_account(account_id)
        .await
        .map_err(|error| error.to_string())?;
    state
        .db
        .unbind_codex_managed_accounts(&[account_id.to_string()])
        .map_err(|error| {
            format!("账号已删除，但 Codex 供应商解绑失败；请编辑供应商重新选择账号或切换供应商: {error}")
        })?;
    Ok(())
}
