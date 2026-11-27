//! Host-owned authentication manager access. No event or orchestration responsibilities.
use crate::proxy::providers::{
    codex_oauth_auth::CodexOAuthManager, copilot_auth::CopilotAuthManager,
    xai_oauth_auth::XaiOAuthManager,
};
use std::sync::Arc;
use tokio::sync::RwLock;

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
