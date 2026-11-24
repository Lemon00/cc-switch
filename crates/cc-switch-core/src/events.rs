//! Frontend-facing hooks for the backend core.
//!
//! The core never touches Tauri directly. A frontend that wants proxy status
//! events, OAuth-manager access or failover switching implements these traits.
//!
//! - GUI（`src-tauri/src/frontend_hooks.rs`）：Tauri `emit` sink + 全部 OAuth
//!   管理器 + `failover_notify` 回调（Tauri State 不是 `Arc`，拿不到 Weak）。
//! - CLI（`crates/cc-switch-cli`）：stderr sink + 自有 `AppState` 里的 Codex
//!   manager + `failover_state`（`Weak<AppState>`）；Copilot/xAI 为 None，
//!   对应路由的供应商直接报错，与旧的无 AppHandle 行为一致。
//!
//! 换句话说：只有 `FrontendHooks::default()` 才是「全空实现」，CLI 用它就会
//! 静默失去故障转移与 OAuth 路由，不要在 CLI/TUI 里用 default。

use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use crate::proxy::providers::copilot_auth::CopilotAuthManager;
use crate::proxy::providers::xai_oauth_auth::XaiOAuthManager;
use crate::store::AppState;
use std::sync::{Arc, Weak};
use tokio::sync::RwLock;

/// Sink for proxy status events (the GUI forwards these to the frontend).
pub trait EventSink: Send + Sync {
    fn emit(&self, event: &str, payload: serde_json::Value);
}

/// No-op sink：只给「明确不需要事件」的场景（测试、无头工具）用，
/// CLI 不用它，否则代理状态事件会被静默丢弃。
pub struct NoOpEventSink;

impl EventSink for NoOpEventSink {
    fn emit(&self, _event: &str, _payload: serde_json::Value) {}
}

/// Access to OAuth managers that live in the frontend's state.
///
/// - GUI：从 Tauri State 里取（`src-tauri/src/frontend_hooks.rs`）；
/// - CLI：从自己的 `AppState::codex_oauth_manager` 取，Copilot/xAI 返回
///   None（CLI 没有对应登录流程）。
pub trait OAuthStateProvider: Send + Sync {
    fn codex_oauth(&self) -> Option<Arc<CodexOAuthManager>>;
    fn copilot_oauth(&self) -> Option<Arc<RwLock<CopilotAuthManager>>>;
    fn xai_oauth(&self) -> Option<Arc<RwLock<XaiOAuthManager>>>;
}

/// Provider that supplies nothing; OAuth-routed providers simply fail the
/// same way they did when the GUI ran without an AppHandle.
pub struct EmptyOAuthStateProvider;

impl OAuthStateProvider for EmptyOAuthStateProvider {
    fn codex_oauth(&self) -> Option<Arc<CodexOAuthManager>> {
        None
    }
    fn copilot_oauth(&self) -> Option<Arc<RwLock<CopilotAuthManager>>> {
        None
    }
    fn xai_oauth(&self) -> Option<Arc<RwLock<XaiOAuthManager>>> {
        None
    }
}

/// Everything the proxy layer needs from a frontend, in one cloneable bag.
/// The GUI fills all of it（含 `failover_notify`）；CLI 用
/// [`FrontendHooks::with_state`] 填 sink/oauth 与 `failover_state`。
#[derive(Clone)]
pub struct FrontendHooks {
    pub sink: Arc<dyn EventSink>,
    pub oauth: Arc<dyn OAuthStateProvider>,
    pub tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Weak back-reference to the frontend's AppState, set after the state
    /// is wrapped in an Arc. Weak on purpose: AppState owns ProxyService,
    /// which owns the server, which owns these hooks.
    pub failover_state: Option<Weak<AppState>>,
    /// 故障转移通知（GUI 用）：设置后，forwarder 不再自己跑 try_switch，
    /// 而是把 (app_type, provider_id, provider_name) 交给前端；前端用自己
    /// 的 AppState（Tauri State 里拿，不经过 Arc）调
    /// `failover_switch::request_failover_switch` 执行切换。
    /// 起因：Tauri 的 StateManager 存的是 Pin<Box<dyn Any>>，不是 Arc，
    /// GUI 根本拿不出 Weak<AppState>，只能走回调；CLI/TUI 有 `Arc<AppState>`，
    /// 直接设 `failover_state` 即可（两者互斥：notify 优先）。
    pub failover_notify: Option<Arc<dyn Fn(&str, &str, &str) + Send + Sync>>,
}

impl Default for FrontendHooks {
    fn default() -> Self {
        Self {
            sink: Arc::new(NoOpEventSink),
            oauth: Arc::new(EmptyOAuthStateProvider),
            tray_refresh: None,
            failover_state: None,
            failover_notify: None,
        }
    }
}

impl FrontendHooks {
    /// 无托盘、无前端回调时用 `Weak<AppState>` 直连故障转移的实现。
    /// CLI/TUI 都走这里：只要 `state` 活得比 hooks 久，forwarder 就能自己
    /// 调 `failover_manager.try_switch`，不必像 GUI 那样绕回调。
    pub fn with_state(
        sink: Arc<dyn EventSink>,
        oauth: Arc<dyn OAuthStateProvider>,
        state: &Arc<AppState>,
    ) -> Self {
        Self {
            sink,
            oauth,
            tray_refresh: None,
            failover_state: Some(Arc::downgrade(state)),
            failover_notify: None,
        }
    }
}
