//! Composition of host capabilities. Failover execution belongs to core;
//! hosts only supply event delivery, authentication and tray presentation.
use crate::{
    auth_state::{EmptyOAuthStateProvider, OAuthStateProvider},
    error::AppError,
    events::{EventSink, NoOpEventSink},
    store::AppState,
};
use std::{future::Future, pin::Pin, sync::Arc};

pub struct FailoverRequest {
    pub app_type: String,
    pub provider_id: String,
    pub provider_name: String,
    pub route_revision: u64,
}

pub type FailoverFuture<'a> = Pin<Box<dyn Future<Output = Result<bool, AppError>> + Send + 'a>>;
pub trait FailoverExecutor: Send + Sync {
    fn execute(&self, request: FailoverRequest) -> FailoverFuture<'_>;
}
struct NoOpFailoverExecutor;
impl FailoverExecutor for NoOpFailoverExecutor {
    fn execute(&self, _: FailoverRequest) -> FailoverFuture<'_> {
        Box::pin(async { Ok(false) })
    }
}

#[derive(Clone)]
pub struct FrontendHooks {
    pub sink: Arc<dyn EventSink>,
    pub oauth: Arc<dyn OAuthStateProvider>,
    pub tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
    pub failover: Arc<dyn FailoverExecutor>,
}
impl Default for FrontendHooks {
    fn default() -> Self {
        Self {
            sink: Arc::new(NoOpEventSink),
            oauth: Arc::new(EmptyOAuthStateProvider),
            tray_refresh: None,
            failover: Arc::new(NoOpFailoverExecutor),
        }
    }
}
impl FrontendHooks {
    pub fn with_state(
        sink: Arc<dyn EventSink>,
        oauth: Arc<dyn OAuthStateProvider>,
        state: &AppState,
    ) -> Self {
        Self::with_state_and_tray(sink, oauth, None, state)
    }
    pub fn with_state_and_tray(
        sink: Arc<dyn EventSink>,
        oauth: Arc<dyn OAuthStateProvider>,
        tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
        state: &AppState,
    ) -> Self {
        let failover = Arc::new(crate::services::failover::StateFailoverExecutor::new(
            state,
            sink.clone(),
            tray_refresh.clone(),
        ));
        Self {
            sink,
            oauth,
            tray_refresh,
            failover,
        }
    }
}
