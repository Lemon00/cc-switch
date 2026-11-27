//! GUI 前端给核心的钩子实现：事件转发、OAuth 状态读取、核心故障转移执行器。
//!
//! 核心（cc-switch-core）不碰 Tauri；这些实现把 Tauri 的 AppHandle
//! 适配成核心的 EventSink / OAuthStateProvider。

use cc_switch_core::events::{EventSink, FrontendHooks, OAuthStateProvider};
use cc_switch_core::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use cc_switch_core::proxy::providers::copilot_auth::CopilotAuthManager;
use cc_switch_core::proxy::providers::xai_oauth_auth::XaiOAuthManager;
use cc_switch_core::store::AppState;
use std::sync::Arc;
use tauri::{Emitter, Manager};
use tokio::sync::RwLock;

/// 把核心事件转发给前端（app.emit）。
pub struct TauriEventSink(pub tauri::AppHandle);

impl EventSink for TauriEventSink {
    fn emit(&self, event: &str, payload: serde_json::Value) {
        let _ = self.0.emit(event, payload);
    }
}

/// 从 Tauri State 里读 OAuth 管理器（forwarder 经此拿到 Codex/Copilot/xAI
/// 的 OAuth 凭据；这些 State 结构体本身只是核心管理器的 Arc 包装）。
pub struct TauriOAuthStateProvider(pub tauri::AppHandle);

impl OAuthStateProvider for TauriOAuthStateProvider {
    fn codex_oauth(&self) -> Option<Arc<CodexOAuthManager>> {
        self.0
            .try_state::<crate::commands::codex_oauth::CodexOAuthState>()
            .map(|s| s.0.clone())
    }

    fn copilot_oauth(&self) -> Option<Arc<RwLock<CopilotAuthManager>>> {
        self.0
            .try_state::<crate::commands::copilot::CopilotAuthState>()
            .map(|s| s.0.clone())
    }

    fn xai_oauth(&self) -> Option<Arc<RwLock<XaiOAuthManager>>> {
        self.0
            .try_state::<crate::commands::xai_oauth::XaiOAuthState>()
            .map(|s| s.0.clone())
    }
}

/// 组装一套完整的 GUI 钩子：事件 sink、OAuth 读取、托盘刷新、核心故障转移执行器。
pub fn tauri_hooks(app: &tauri::AppHandle, state: &AppState) -> FrontendHooks {
    let sink: Arc<dyn EventSink> = Arc::new(TauriEventSink(app.clone()));
    let tray_app = app.clone();
    let tray_refresh: Option<Arc<dyn Fn() + Send + Sync>> = Some(Arc::new(move || {
        crate::tray::refresh_tray_menu(&tray_app);
    }));
    FrontendHooks::with_state_and_tray(
        sink,
        Arc::new(TauriOAuthStateProvider(app.clone())),
        tray_refresh,
        state,
    )
}
