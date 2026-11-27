use crate::database::Database;
use crate::proxy::providers::codex_oauth_auth::CodexOAuthManager;
use crate::services::{ProxyService, UsageCache};
use std::ops::Deref;
use std::sync::{Arc, Weak};

/// 全局应用状态
#[derive(Clone)]
pub struct AppState(Arc<AppStateInner>);

/// Shared resources have one lifetime regardless of the host (Tauri or CLI).
pub struct AppStateInner {
    pub db: Arc<Database>,
    pub failover_manager: crate::proxy::failover_switch::FailoverSwitchManager,
    pub operations: crate::coordination::AppOperationLocks,
    pub proxy_service: ProxyService,
    pub usage_cache: Arc<UsageCache>,
    // 内部已使用细粒度锁（accounts/access_tokens/refresh_locks），所有方法均为
    // `&self`，无需外层 RwLock；避免持有粗粒度锁跨网络刷新导致的连锁阻塞。
    pub codex_oauth_manager: Arc<CodexOAuthManager>,
}

impl AppState {
    /// 创建新的应用状态
    pub fn new(db: Arc<Database>) -> Self {
        let codex_oauth_manager =
            Arc::new(CodexOAuthManager::new(crate::config::get_app_config_dir()));
        let proxy_service = ProxyService::new(db.clone());

        Self(Arc::new(AppStateInner {
            db,
            failover_manager: crate::proxy::failover_switch::FailoverSwitchManager::new(),
            operations: crate::coordination::AppOperationLocks::new(),
            proxy_service,
            usage_cache: Arc::new(UsageCache::new()),
            codex_oauth_manager,
        }))
    }
}

impl Deref for AppState {
    type Target = AppStateInner;
    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

#[derive(Clone)]
pub struct WeakAppState(Weak<AppStateInner>);
impl WeakAppState {
    pub fn upgrade(&self) -> Option<AppState> {
        self.0.upgrade().map(AppState)
    }
}
impl AppState {
    pub fn downgrade(&self) -> WeakAppState {
        WeakAppState(Arc::downgrade(&self.0))
    }
}
