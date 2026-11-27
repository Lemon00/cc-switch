//! 熔断器模块
//!
//! 实现熔断器模式，用于防止向不健康的供应商发送请求

use super::log_codes::cb as log_cb;
use serde::{Deserialize, Serialize};
use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Instant;
use tokio::sync::RwLock;

/// 计算 Closed 态错误率时保留的最近请求结果数量（成功/失败各记 1 bit）。
// Covers the full 5..=100 minimum-sample range exposed by the settings panel.
const ERROR_RATE_WINDOW_CAP: usize = 100;

/// 固定容量环形缓冲：只用于 Closed 态的错误率判定（连续失败阈值仍用原子计数）。
#[derive(Debug)]
struct ErrorRateWindow {
    slots: [bool; ERROR_RATE_WINDOW_CAP],
    head: usize,
    len: usize,
}

impl Default for ErrorRateWindow {
    fn default() -> Self {
        Self {
            slots: [false; ERROR_RATE_WINDOW_CAP],
            head: 0,
            len: 0,
        }
    }
}

impl ErrorRateWindow {
    fn reset(&mut self) {
        self.head = 0;
        self.len = 0;
        self.slots = [false; ERROR_RATE_WINDOW_CAP];
    }

    fn push(&mut self, failure: bool) {
        self.slots[self.head] = failure;
        self.head = (self.head + 1) % ERROR_RATE_WINDOW_CAP;
        if self.len < ERROR_RATE_WINDOW_CAP {
            self.len += 1;
        }
    }

    fn failure_count(&self) -> usize {
        if self.len == ERROR_RATE_WINDOW_CAP {
            self.slots.iter().filter(|&&failed| failed).count()
        } else {
            self.slots[..self.len]
                .iter()
                .filter(|&&failed| failed)
                .count()
        }
    }
}

/// 熔断器状态
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CircuitState {
    /// 关闭状态 - 正常工作
    Closed,
    /// 打开状态 - 熔断激活，拒绝请求
    Open,
    /// 半开状态 - 尝试恢复，允许部分请求通过
    HalfOpen,
}

impl std::fmt::Display for CircuitState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            CircuitState::Closed => write!(f, "closed"),
            CircuitState::Open => write!(f, "open"),
            CircuitState::HalfOpen => write!(f, "half_open"),
        }
    }
}

pub use crate::proxy_config::CircuitBreakerConfig;

/// 熔断器实例
pub struct CircuitBreaker {
    /// 当前状态
    state: Arc<RwLock<CircuitState>>,
    /// 连续失败计数
    consecutive_failures: Arc<AtomicU32>,
    /// 连续成功计数（半开状态）
    consecutive_successes: Arc<AtomicU32>,
    /// 总请求计数
    total_requests: Arc<AtomicU32>,
    /// 失败请求计数
    failed_requests: Arc<AtomicU32>,
    /// 上次打开时间
    last_opened_at: Arc<RwLock<Option<Instant>>>,
    /// 配置（支持热更新）
    config: Arc<RwLock<CircuitBreakerConfig>>,
    /// 半开探测名额：高 32 位是第几轮 HalfOpen，低 32 位是这一轮已放出的名额数。
    /// 两者放在一个原子里，归还时才能确认名额属于当前这一轮：上一轮的探测晚到归还，
    /// 不能把这一轮正在进行的探测名额也还掉。
    half_open_slots: Arc<AtomicU64>,
    /// 日志里标明是哪家的熔断器（路由器传 `app_type:provider_id`）
    label: String,
    /// Closed 态滑动窗口（在 `state` 写锁持有期间更新，避免与状态迁移竞态）
    error_rate_window: Mutex<ErrorRateWindow>,
}

/// 熔断器放行结果
///
/// `used_half_open_permit` 表示本次放行是否占用了 HalfOpen 探测名额。
/// 调用方应在请求结束后把该值传回 `record_success` / `record_failure` 用于正确释放名额。
#[derive(Debug, Clone, Copy)]
pub struct AllowResult {
    pub allowed: bool,
    pub used_half_open_permit: bool,
    /// 名额属于第几轮 HalfOpen（只在 `used_half_open_permit` 时有意义）
    pub(crate) half_open_round: u32,
}

/// 持有一个 HalfOpen 探测名额，drop 时归还。
///
/// 名额若只靠请求结束时的 `record_*` 归还，请求中途被丢弃（客户端断开取消了 future、
/// panic）就永远还不回来：计数卡在上限，这家一直停在 HalfOpen、每个请求都被拒绝，
/// 直到重启（#7206 / #9292）。拿到名额后立即交给它，任何退出路径都会归还。
/// 只归还它自己那一轮的名额：熔断器已经进入新一轮 HalfOpen（计数已清零）时什么都不做。
pub struct HalfOpenPermitGuard {
    breaker: Arc<CircuitBreaker>,
    round: u32,
}

impl HalfOpenPermitGuard {
    pub(crate) fn new(breaker: Arc<CircuitBreaker>, round: u32) -> Self {
        Self { breaker, round }
    }
}

impl Drop for HalfOpenPermitGuard {
    fn drop(&mut self) {
        self.breaker.release_half_open_slot(Some(self.round));
    }
}

impl CircuitBreaker {
    /// 创建新的熔断器
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            state: Arc::new(RwLock::new(CircuitState::Closed)),
            consecutive_failures: Arc::new(AtomicU32::new(0)),
            consecutive_successes: Arc::new(AtomicU32::new(0)),
            total_requests: Arc::new(AtomicU32::new(0)),
            failed_requests: Arc::new(AtomicU32::new(0)),
            last_opened_at: Arc::new(RwLock::new(None)),
            config: Arc::new(RwLock::new(config)),
            half_open_slots: Arc::new(AtomicU64::new(0)),
            label: String::new(),
            error_rate_window: Mutex::new(ErrorRateWindow::default()),
        }
    }

    fn reset_error_rate_window(&self) {
        if let Ok(mut window) = self.error_rate_window.lock() {
            window.reset();
        }
    }

    fn push_closed_outcome(&self, failure: bool) {
        if let Ok(mut window) = self.error_rate_window.lock() {
            window.push(failure);
        }
    }

    fn closed_window_error_rate(&self) -> (usize, usize) {
        self.error_rate_window
            .lock()
            .map(|w| (w.failure_count(), w.len))
            .unwrap_or((0, 0))
    }

    /// 设置日志标识，状态变化的日志会带上它，多家同时熔断时才分得清是哪家。
    pub fn with_label(mut self, label: impl Into<String>) -> Self {
        self.label = label.into();
        self
    }

    /// 更新熔断器配置（热更新，不重置状态）
    pub async fn update_config(&self, new_config: CircuitBreakerConfig) {
        *self.config.write().await = new_config;
    }

    /// 判断当前 Provider 是否“可被纳入候选链路”
    ///
    /// 与 [`Self::is_selectable`] 相同；保留此名称以兼容既有调用点。
    pub async fn is_available(&self) -> bool {
        self.is_selectable().await
    }

    /// 路由选择阶段的可用性判断（不占用 HalfOpen 探测名额）：
    /// - Closed：可选
    /// - Open：超时到达后切到 HalfOpen，再按探测余量判断
    /// - HalfOpen：仅当仍有探测名额时可选
    ///
    /// 真正发起请求前仍需调用 `allow_request()` 获取名额。
    pub async fn is_selectable(&self) -> bool {
        let state = self.state_for_request().await;
        match *state {
            CircuitState::Closed => true,
            CircuitState::HalfOpen => self.half_open_probe_capacity_available(),
            CircuitState::Open => false,
        }
    }

    /// Keep the state locked through cooldown validation and permit acquisition.
    /// All nested locks follow state -> last_opened_at, just like record_failure.
    async fn state_for_request(&self) -> tokio::sync::RwLockWriteGuard<'_, CircuitState> {
        let timeout_seconds = self.config.read().await.timeout_seconds;
        let mut state = self.state.write().await;
        if *state == CircuitState::Open {
            let opened_at = *self.last_opened_at.read().await;
            if opened_at.is_some_and(|at| at.elapsed().as_secs() >= timeout_seconds) {
                self.enter_half_open(&mut state);
                log::info!(
                    "[{}] 熔断器 [{}] Open → HalfOpen (超时恢复)",
                    log_cb::OPEN_TO_HALF_OPEN,
                    self.label
                );
            }
        }
        state
    }

    /// 检查是否允许请求通过
    pub async fn allow_request(&self) -> AllowResult {
        let state = self.state_for_request().await;
        match *state {
            CircuitState::Closed => AllowResult {
                allowed: true,
                used_half_open_permit: false,
                half_open_round: 0,
            },
            CircuitState::Open => AllowResult {
                allowed: false,
                used_half_open_permit: false,
                half_open_round: 0,
            },
            CircuitState::HalfOpen => self.allow_half_open_probe(),
        }
    }

    /// 记录成功
    pub async fn record_success(&self, used_half_open_permit: bool) {
        if used_half_open_permit {
            self.release_half_open_permit();
        }

        let config = self.config.read().await.clone();
        let mut state_guard = self.state.write().await;

        self.consecutive_failures.store(0, Ordering::SeqCst);
        self.total_requests.fetch_add(1, Ordering::SeqCst);

        match *state_guard {
            CircuitState::Closed => {
                self.push_closed_outcome(false);
            }
            CircuitState::HalfOpen => {
                let successes = self.consecutive_successes.fetch_add(1, Ordering::SeqCst) + 1;
                if successes >= config.success_threshold {
                    log::info!(
                        "[{}] 熔断器 [{}] HalfOpen → Closed (恢复正常)",
                        log_cb::HALF_OPEN_TO_CLOSED,
                        self.label
                    );
                    *state_guard = CircuitState::Closed;
                    self.consecutive_failures.store(0, Ordering::SeqCst);
                    self.consecutive_successes.store(0, Ordering::SeqCst);
                    self.total_requests.store(0, Ordering::SeqCst);
                    self.failed_requests.store(0, Ordering::SeqCst);
                    self.reset_error_rate_window();
                }
            }
            CircuitState::Open => {}
        }
    }

    /// 记录失败
    pub async fn record_failure(&self, used_half_open_permit: bool) {
        if used_half_open_permit {
            self.release_half_open_permit();
        }

        let config = self.config.read().await.clone();
        let mut state_guard = self.state.write().await;

        let failures = self.consecutive_failures.fetch_add(1, Ordering::SeqCst) + 1;
        self.total_requests.fetch_add(1, Ordering::SeqCst);
        self.failed_requests.fetch_add(1, Ordering::SeqCst);
        self.consecutive_successes.store(0, Ordering::SeqCst);

        let should_open = match *state_guard {
            CircuitState::HalfOpen => {
                log::warn!(
                    "[{}] 熔断器 [{}] HalfOpen 探测失败 → Open",
                    log_cb::HALF_OPEN_PROBE_FAILED,
                    self.label
                );
                true
            }
            CircuitState::Closed => {
                self.push_closed_outcome(true);
                if failures >= config.failure_threshold {
                    log::warn!(
                        "[{}] 熔断器 [{}] 触发: 连续失败 {failures} 次 → Open",
                        log_cb::TRIGGERED_FAILURES,
                        self.label
                    );
                    true
                } else {
                    let (failed, total) = self.closed_window_error_rate();
                    if total >= config.min_requests as usize {
                        let error_rate = failed as f64 / total as f64;
                        if error_rate >= config.error_rate_threshold {
                            log::warn!(
                                "[{}] 熔断器 [{}] 触发: 错误率 {:.1}% → Open",
                                log_cb::TRIGGERED_ERROR_RATE,
                                self.label,
                                error_rate * 100.0
                            );
                            true
                        } else {
                            false
                        }
                    } else {
                        false
                    }
                }
            }
            CircuitState::Open => false,
        };

        if should_open {
            *state_guard = CircuitState::Open;
            *self.last_opened_at.write().await = Some(Instant::now());
            self.consecutive_failures.store(0, Ordering::SeqCst);
            self.consecutive_successes.store(0, Ordering::SeqCst);
            self.reset_error_rate_window();
        }
    }

    /// 获取当前状态
    #[allow(dead_code)]
    pub async fn get_state(&self) -> CircuitState {
        *self.state.read().await
    }

    /// 获取统计信息
    #[allow(dead_code)]
    pub async fn get_stats(&self) -> CircuitBreakerStats {
        CircuitBreakerStats {
            state: *self.state.read().await,
            consecutive_failures: self.consecutive_failures.load(Ordering::SeqCst),
            consecutive_successes: self.consecutive_successes.load(Ordering::SeqCst),
            total_requests: self.total_requests.load(Ordering::SeqCst),
            failed_requests: self.failed_requests.load(Ordering::SeqCst),
        }
    }

    /// 重置熔断器（手动恢复）
    #[allow(dead_code)]
    pub async fn reset(&self) {
        log::info!(
            "[{}] 熔断器 [{}] 手动重置 → Closed",
            log_cb::MANUAL_RESET,
            self.label
        );
        self.transition_to_closed().await;
    }

    /// 半开探测是否仍有名额（只读，不占用）。
    fn half_open_probe_capacity_available(&self) -> bool {
        const MAX_HALF_OPEN_REQUESTS: u32 = 1;
        let slots = self.half_open_slots.load(Ordering::SeqCst);
        let (_, in_flight) = split_half_open_slots(slots);
        in_flight < MAX_HALF_OPEN_REQUESTS
    }

    fn allow_half_open_probe(&self) -> AllowResult {
        // 半开状态限流：只允许有限请求通过进行探测
        let max_half_open_requests = 1u32;
        let previous = self.half_open_slots.fetch_add(1, Ordering::SeqCst);
        let (round, current) = split_half_open_slots(previous);

        if current < max_half_open_requests {
            AllowResult {
                allowed: true,
                used_half_open_permit: true,
                half_open_round: round,
            }
        } else {
            // 超过限额，回退计数，拒绝请求
            self.release_half_open_slot(Some(round));
            AllowResult {
                allowed: false,
                used_half_open_permit: false,
                half_open_round: 0,
            }
        }
    }

    /// 仅释放 HalfOpen permit，不影响健康统计
    ///
    /// 用于整流器等场景：请求结果不应计入 Provider 健康度，
    /// 但仍需释放占用的探测名额，避免 HalfOpen 状态卡死
    pub fn release_half_open_permit(&self) {
        self.release_half_open_slot(None);
    }

    /// 归还一个名额。`round` 给定时只在仍是那一轮时归还；计数已为 0 时不动。
    fn release_half_open_slot(&self, round: Option<u32>) {
        let _ = self
            .half_open_slots
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |slots| {
                let (current_round, count) = split_half_open_slots(slots);
                if count == 0 || round.is_some_and(|round| round != current_round) {
                    None
                } else {
                    Some(slots - 1)
                }
            });
    }

    /// 转换到打开状态
    async fn transition_to_open(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Open;
        *self.last_opened_at.write().await = Some(Instant::now());
        self.consecutive_failures.store(0, Ordering::SeqCst);
        self.consecutive_successes.store(0, Ordering::SeqCst);
        self.reset_error_rate_window();
    }

    /// 转换到半开状态
    async fn transition_to_half_open(&self) {
        let mut state = self.state.write().await;
        self.enter_half_open(&mut state);
    }

    fn enter_half_open(&self, state: &mut CircuitState) {
        if *state != CircuitState::Open {
            return;
        }

        *state = CircuitState::HalfOpen;
        self.consecutive_successes.store(0, Ordering::SeqCst);
        // 进入新一轮：轮次加一、名额计数清零（上一轮没还的名额就此作废）
        let _ = self
            .half_open_slots
            .fetch_update(Ordering::SeqCst, Ordering::SeqCst, |slots| {
                let (round, _) = split_half_open_slots(slots);
                Some(u64::from(round.wrapping_add(1)) << 32)
            });
    }

    /// 转换到关闭状态
    async fn transition_to_closed(&self) {
        let mut state = self.state.write().await;
        *state = CircuitState::Closed;
        self.consecutive_failures.store(0, Ordering::SeqCst);
        self.consecutive_successes.store(0, Ordering::SeqCst);
        // 重置计数器
        self.total_requests.store(0, Ordering::SeqCst);
        self.failed_requests.store(0, Ordering::SeqCst);
        self.reset_error_rate_window();
    }
}

/// 拆出 `half_open_slots` 的（轮次，已放出名额数）。
fn split_half_open_slots(slots: u64) -> (u32, u32) {
    ((slots >> 32) as u32, slots as u32)
}

/// 熔断器统计信息
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CircuitBreakerStats {
    pub state: CircuitState,
    pub consecutive_failures: u32,
    pub consecutive_successes: u32,
    pub total_requests: u32,
    pub failed_requests: u32,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn maximum_configured_sample_count_can_open_the_circuit() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 20,
            error_rate_threshold: 0.5,
            min_requests: 100,
            ..Default::default()
        });
        for _ in 0..49 {
            breaker.record_success(false).await;
            breaker.record_failure(false).await;
        }
        breaker.record_success(false).await;
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        breaker.record_failure(false).await;
        assert_eq!(breaker.get_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn cooldown_checks_do_not_deadlock_with_a_concurrent_failure() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            timeout_seconds: 0,
            ..Default::default()
        });
        breaker.transition_to_open().await;
        // Force the selector to wait on the timestamp, then enqueue a failure.
        // The previous lock order held last_opened_at while waiting for state,
        // opposite to record_failure, and could deadlock both requests.
        let opened_at = breaker.last_opened_at.write().await;
        let select = breaker.is_selectable();
        tokio::pin!(select);
        assert!(matches!(
            futures::poll!(&mut select),
            std::task::Poll::Pending
        ));
        let failure = async {
            let mut state = breaker.state.write().await;
            *state = CircuitState::Open;
            *breaker.last_opened_at.write().await = Some(Instant::now());
        };
        tokio::pin!(failure);
        assert!(matches!(
            futures::poll!(&mut failure),
            std::task::Poll::Pending
        ));
        drop(opened_at);
        tokio::time::timeout(std::time::Duration::from_secs(1), async {
            tokio::join!(select, failure);
        })
        .await
        .expect("cooldown and failure must finish");
    }

    #[tokio::test]
    async fn test_circuit_breaker_closed_to_open() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // 初始状态应该是关闭
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        assert!(breaker.allow_request().await.allowed);

        // 记录 3 次失败
        for _ in 0..3 {
            breaker.record_failure(false).await;
        }

        // 应该转换到打开状态
        assert_eq!(breaker.get_state().await, CircuitState::Open);
        assert!(!breaker.allow_request().await.allowed);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_to_closed() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // 打开熔断器
        breaker.record_failure(false).await;
        breaker.record_failure(false).await;
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // 手动转换到半开状态
        breaker.transition_to_half_open().await;
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);

        // 记录 2 次成功
        breaker.record_success(false).await;
        breaker.record_success(false).await;

        // 应该转换到关闭状态
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
    }

    #[tokio::test]
    async fn test_is_selectable_respects_half_open_probe_capacity() {
        let config = CircuitBreakerConfig {
            timeout_seconds: 0,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        breaker.transition_to_open().await;
        let first = breaker.allow_request().await;
        assert!(first.allowed);
        assert!(first.used_half_open_permit);
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);
        assert!(!breaker.is_selectable().await);

        breaker.release_half_open_permit();
        assert!(breaker.is_selectable().await);
    }

    #[tokio::test]
    async fn test_half_open_transition_does_not_reset_inflight_permit() {
        let config = CircuitBreakerConfig {
            timeout_seconds: 0,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // 进入 Open，然后由于 timeout_seconds=0，allow_request 会立即切换到 HalfOpen 并占用探测名额
        breaker.transition_to_open().await;
        let first = breaker.allow_request().await;
        assert!(first.allowed);
        assert!(first.used_half_open_permit);
        assert_eq!(breaker.get_state().await, CircuitState::HalfOpen);

        // 模拟并发下的“重复 HalfOpen 转换调用”，不应重置 in-flight 计数
        breaker.transition_to_half_open().await;

        // 由于名额仍被占用，第二次请求应被拒绝
        let second = breaker.allow_request().await;
        assert!(!second.allowed);
        assert!(!second.used_half_open_permit);
    }

    #[tokio::test]
    async fn test_error_rate_uses_sliding_window_not_lifetime_totals() {
        let config = CircuitBreakerConfig {
            failure_threshold: 100,
            error_rate_threshold: 0.5,
            min_requests: 10,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // 先填满窗口的成功样本，再集中失败：窗口内 50%，全量累计仅约 33%
        for _ in 0..ERROR_RATE_WINDOW_CAP {
            breaker.record_success(false).await;
        }
        assert_eq!(breaker.get_state().await, CircuitState::Closed);

        for _ in 0..ERROR_RATE_WINDOW_CAP / 2 {
            breaker.record_failure(false).await;
        }
        assert_eq!(breaker.get_state().await, CircuitState::Open);
    }

    #[tokio::test]
    async fn test_circuit_breaker_reset() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            ..Default::default()
        };
        let breaker = CircuitBreaker::new(config);

        // 打开熔断器
        breaker.record_failure(false).await;
        breaker.record_failure(false).await;
        assert_eq!(breaker.get_state().await, CircuitState::Open);

        // 重置
        breaker.reset().await;
        assert_eq!(breaker.get_state().await, CircuitState::Closed);
        assert!(breaker.allow_request().await.allowed);
    }
}
