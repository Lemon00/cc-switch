//! Host-owned authentication manager access. No event or orchestration responsibilities.
use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use std::sync::Arc;

///   None（CLI 没有对应登录流程）。
pub trait OAuthStateProvider: Send + Sync {
    fn codex_oauth(&self) -> Option<Arc<CodexOAuthManager>>;
}

/// Provider that supplies nothing; OAuth-routed providers simply fail the
/// same way they did when the GUI ran without an AppHandle.
pub struct EmptyOAuthStateProvider;

impl OAuthStateProvider for EmptyOAuthStateProvider {
    fn codex_oauth(&self) -> Option<Arc<CodexOAuthManager>> {
        None
    }
}
