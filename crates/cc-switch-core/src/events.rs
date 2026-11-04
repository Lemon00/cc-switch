//! Frontend-facing hooks for the backend core.
//!
//! The core never touches Tauri directly. A frontend that wants proxy status
//! events or OAuth-manager access implements these traits; the CLI uses the
//! no-op / None variants.

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

/// No-op sink used by the CLI.
pub struct NoOpEventSink;

impl EventSink for NoOpEventSink {
    fn emit(&self, _event: &str, _payload: serde_json::Value) {}
}

/// Access to OAuth managers that live in the frontend's state in the GUI
/// build (Tauri State). The CLI provides the Codex manager from its own
/// AppState and None for the others.
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
/// The GUI fills all of it; the CLI fills only the failover state (and
/// leaves sink/oauth at their defaults).
#[derive(Clone)]
pub struct FrontendHooks {
    pub sink: Arc<dyn EventSink>,
    pub oauth: Arc<dyn OAuthStateProvider>,
    pub tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
    /// Weak back-reference to the frontend's AppState, set after the state
    /// is wrapped in an Arc. Weak on purpose: AppState owns ProxyService,
    /// which owns the server, which owns these hooks.
    pub failover_state: Option<Weak<AppState>>,
}

impl Default for FrontendHooks {
    fn default() -> Self {
        Self {
            sink: Arc::new(NoOpEventSink),
            oauth: Arc::new(EmptyOAuthStateProvider),
            tray_refresh: None,
            failover_state: None,
        }
    }
}
