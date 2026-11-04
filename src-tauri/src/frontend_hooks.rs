//! GUI 前端给核心的钩子实现：事件转发、OAuth 状态读取、故障转移回调。
//!
//! 核心（cc-switch-core）不碰 Tauri；这些实现把 Tauri 的 AppHandle
//! 适配成核心的 EventSink / OAuthStateProvider / failover_notify。

use cc_switch_core::events::{EventSink, FrontendHooks, OAuthStateProvider};
use cc_switch_core::proxy::failover_switch::request_failover_switch;
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

/// 组装一套完整的 GUI 钩子：事件 sink、OAuth 读取、托盘刷新、故障转移回调。
pub fn tauri_hooks(app: &tauri::AppHandle) -> FrontendHooks {
    let sink: Arc<dyn EventSink> = Arc::new(TauriEventSink(app.clone()));
    let tray_app = app.clone();
    let tray_refresh: Option<Arc<dyn Fn() + Send + Sync>> =
        Some(Arc::new(move || {
            crate::tray::refresh_tray_menu(&tray_app);
        }));
    let failover_notify = tauri_failover_notify(app.clone(), sink.clone(), tray_refresh.clone());
    FrontendHooks {
        sink,
        oauth: Arc::new(TauriOAuthStateProvider(app.clone())),
        tray_refresh,
        failover_state: None,
        failover_notify: Some(failover_notify),
    }
}

/// 故障转移回调：forwarder 调它时只传三个 id；这里在 Tauri 任务里从
/// State 取 AppState，再调核心的 request_failover_switch 执行切换。
/// 不直接用 Weak<AppState>，因为 Tauri StateManager 存的不是 Arc。
fn tauri_failover_notify(
    app: tauri::AppHandle,
    sink: Arc<dyn EventSink>,
    tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
) -> Arc<dyn Fn(&str, &str, &str) + Send + Sync> {
    Arc::new(move |app_type, provider_id, provider_name| {
        let app = app.clone();
        let sink = sink.clone();
        let tray_refresh = tray_refresh.clone();
        let app_type = app_type.to_string();
        let provider_id = provider_id.to_string();
        let provider_name = provider_name.to_string();
        tauri::async_runtime::spawn(async move {
            let state = app.state::<AppState>();
            let _ = request_failover_switch(
                &state,
                sink.as_ref(),
                tray_refresh,
                &app_type,
                &provider_id,
                &provider_name,
            )
            .await;
        });
    })
}
