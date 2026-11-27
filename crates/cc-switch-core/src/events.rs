//! Application event delivery. Hosts supply their own sink.

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

// Compatibility paths; contracts are defined in their owning modules.
pub use crate::auth_state::{EmptyOAuthStateProvider, OAuthStateProvider};
pub use crate::runtime_hooks::FrontendHooks;
