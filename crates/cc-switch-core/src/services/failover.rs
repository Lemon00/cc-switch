//! A shared failover use case for every host, with weak state ownership.
use crate::{
    events::EventSink,
    runtime_hooks::{FailoverExecutor, FailoverFuture, FailoverRequest},
    store::{AppState, WeakAppState},
};
use std::sync::Arc;

pub struct StateFailoverExecutor {
    state: WeakAppState,
    sink: Arc<dyn EventSink>,
    tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
}
impl StateFailoverExecutor {
    pub fn new(
        state: &AppState,
        sink: Arc<dyn EventSink>,
        tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
    ) -> Self {
        Self {
            state: state.downgrade(),
            sink,
            tray_refresh,
        }
    }
}
impl FailoverExecutor for StateFailoverExecutor {
    fn execute(&self, request: FailoverRequest) -> FailoverFuture<'_> {
        Box::pin(async move {
            let Some(state) = self.state.upgrade() else {
                return Ok(false);
            };
            crate::proxy::failover_switch::request_failover_switch(
                &state,
                self.sink.as_ref(),
                self.tray_refresh.clone(),
                &request.app_type,
                &request.provider_id,
                &request.provider_name,
            )
            .await
        })
    }
}
