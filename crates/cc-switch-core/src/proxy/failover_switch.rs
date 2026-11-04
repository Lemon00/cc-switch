//! 故障转移切换模块
//!
//! 处理故障转移成功后的供应商切换逻辑，包括：
//! - 去重控制（避免多个请求同时触发）
//! - 前端事件发射（通过 EventSink，GUI 转发给前端；CLI 为 no-op）
//! - 托盘刷新钩子（GUI 注入；CLI 不提供）

use crate::error::AppError;
use crate::events::EventSink;
use crate::store::AppState;
use std::collections::HashSet;
use std::sync::{Arc, OnceLock};
use tokio::sync::RwLock;

/// 前端回调路径共用的切换管理器（GUI 的 failover_notify 回调经此执行，
/// 与 forwarder 自带的 per-forwarder 管理器分开——GUI 构建里 forwarder
/// 只走回调，所以去重语义在 GUI 内仍是一致的）。
static GLOBAL_MANAGER: OnceLock<FailoverSwitchManager> = OnceLock::new();

/// 供前端（GUI failover_notify 回调）调用：用前端自己的 AppState 执行
/// 一次故障转移切换。路由提交本身有 mode 锁串行化，即使并发调用，
/// 第二次也会因 proxy_route 已是目标而返回 false，不会重复切换。
pub async fn request_failover_switch(
    state: &AppState,
    sink: &dyn EventSink,
    tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
    app_type: &str,
    provider_id: &str,
    provider_name: &str,
) -> Result<bool, AppError> {
    let manager = GLOBAL_MANAGER.get_or_init(FailoverSwitchManager::new);
    let ctx = FailoverContext {
        state,
        sink: Some(sink),
        tray_refresh,
    };
    manager
        .try_switch(Some(&ctx), app_type, provider_id, provider_name)
        .await
}

/// 故障转移切换时的前端上下文。CLI 只给 state（路由记录照常工作），
/// sink 与 tray_refresh 为空时对应行为跳过。
pub struct FailoverContext<'a> {
    pub state: &'a AppState,
    pub sink: Option<&'a dyn EventSink>,
    pub tray_refresh: Option<Arc<dyn Fn() + Send + Sync>>,
}

/// 故障转移切换管理器
///
/// 负责处理故障转移成功后的供应商切换，确保 UI 能够直观反映当前使用的供应商。
#[derive(Clone, Default)]
pub struct FailoverSwitchManager {
    /// 正在处理中的切换（key = "app_type:provider_id"）
    pending_switches: Arc<RwLock<HashSet<String>>>,
}

impl FailoverSwitchManager {
    pub fn new() -> Self {
        Self {
            pending_switches: Arc::new(RwLock::new(HashSet::new())),
        }
    }

    /// 尝试执行故障转移切换
    ///
    /// # Returns
    /// - `Ok(true)` - 切换成功执行
    /// - `Ok(false)` - 切换已在进行中，跳过
    /// - `Err(e)` - 切换过程中发生错误
    pub async fn try_switch(
        &self,
        ctx: Option<&FailoverContext<'_>>,
        app_type: &str,
        provider_id: &str,
        provider_name: &str,
    ) -> Result<bool, AppError> {
        let switch_key = format!("{app_type}:{provider_id}");

        // 去重检查：如果相同切换已在进行中，跳过
        {
            let mut pending = self.pending_switches.write().await;
            if pending.contains(&switch_key) {
                log::debug!("[Failover] 切换已在进行中，跳过: {app_type} -> {provider_id}");
                return Ok(false);
            }
            pending.insert(switch_key.clone());
        }

        // 执行切换（确保最后清理 pending 标记）
        let result = self
            .do_switch(ctx, app_type, provider_id, provider_name)
            .await;

        // 清理 pending 标记
        {
            let mut pending = self.pending_switches.write().await;
            pending.remove(&switch_key);
        }

        result
    }

    async fn do_switch(
        &self,
        ctx: Option<&FailoverContext<'_>>,
        app_type: &str,
        provider_id: &str,
        provider_name: &str,
    ) -> Result<bool, AppError> {
        // 只有处于代理模式的应用才允许执行故障转移切换
        let Ok(app_enum) = app_type.parse::<crate::app_config::AppType>() else {
            return Ok(false);
        };
        if !crate::mode::current::is_proxy(&app_enum) {
            log::debug!("[Failover] {app_type} 不在路由模式，跳过切换");
            return Ok(false);
        }

        log::info!("[FO-001] 切换: {app_type} → {provider_name}");

        let Some(ctx) = ctx else {
            // 没有前端上下文时无法记录路由，切换不执行（与旧逻辑中
            // app_handle 为 None 时 switched 保持 false 一致）。
            return Ok(false);
        };

        // 只换代理的路由，不写客户端文件。
        let switched = crate::mode::controller::record_failover_route(
            ctx.state,
            &app_enum,
            provider_id,
        )
        .await
        .map_err(AppError::Message)?;

        if !switched {
            return Ok(false);
        }

        // GUI 注入的托盘刷新钩子；CLI 不提供则跳过。
        if let Some(refresh) = &ctx.tray_refresh {
            refresh();
        }

        // 发射事件到前端
        if let Some(sink) = ctx.sink {
            sink.emit(
                "provider-switched",
                serde_json::json!({
                    "appType": app_type,
                    "providerId": provider_id,
                    "source": "failover"  // 标识来源是故障转移
                }),
            );
        }

        Ok(switched)
    }
}
