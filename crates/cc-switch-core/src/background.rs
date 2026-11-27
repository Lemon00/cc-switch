//! Ambient-independent background runtime for fire-and-forget workers.
//!
//! During decoupling, `tauri::async_runtime::spawn` inside the auto-sync
//! `start_worker` functions was mechanically replaced with `tokio::task::spawn`.
//! That broke the GUI: Tauri's `setup` closure runs without a current tokio
//! runtime, so the first `start_worker` call panicked with `TryCurrentError`
//! and the app died on startup (found by the 2026-11-04 headless run test).
//! `tauri::async_runtime::spawn` never had this problem because it spawns on
//! Tauri's global runtime handle, not on an ambient one.
//!
//! This module restores ambient-independence on the core side: workers spawn
//! on a single lazily-built runtime owned by core, so `start_worker` is safe
//! to call from any sync context (GUI setup, CLI, tests) without a runtime
//! already being current.

use std::future::Future;
use std::sync::OnceLock;
use tokio::runtime::{Builder, Handle, Runtime};
use tokio::task::JoinHandle;

fn runtime() -> &'static Runtime {
    static RT: OnceLock<Runtime> = OnceLock::new();
    RT.get_or_init(|| {
        Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("cc-switch-bg")
            .enable_all()
            .build()
            .expect("cc-switch background runtime")
    })
}

/// Let the desktop shell share the core's process-lifetime runtime. Creating a
/// second runtime for Tauri duplicates worker threads even in lightweight mode.
/// Core keeps ownership, so the supplied handle cannot outlive its runtime.
pub fn runtime_handle() -> Handle {
    runtime().handle().clone()
}

/// Spawn a future on core's background runtime, independent of any ambient
/// tokio runtime at the call site. The runtime lives for the process lifetime.
pub fn spawn_background<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(fut)
}

/// 常驻 worker（自动同步循环等）用它：`JoinHandle` 被丢弃时 panic 是完全
/// 静默的，进程看起来正常、同步却再也不跑。这里把退出原因记进日志。
pub fn spawn_background_worker<F>(name: &'static str, fut: F)
where
    F: Future<Output = ()> + Send + 'static,
{
    runtime().spawn(async move {
        // 已经在本 runtime 内，用 tokio::spawn 才能拿到 JoinError。
        if let Err(error) = tokio::spawn(fut).await {
            log::error!("后台任务 {name} 退出: {error}");
        }
    });
}

#[cfg(test)]
mod tests {
    #[test]
    fn shared_handle_runs_core_timers_and_blocking_tasks_without_ambient_runtime() {
        std::thread::spawn(|| {
            assert!(tokio::runtime::Handle::try_current().is_err());
            let handle = super::runtime_handle();
            let task = super::spawn_background(async {
                tokio::time::sleep(std::time::Duration::from_millis(1)).await;
                tokio::task::spawn_blocking(|| 42).await.unwrap()
            });
            assert_eq!(handle.block_on(task).unwrap(), 42);
        })
        .join()
        .unwrap();
    }
}
