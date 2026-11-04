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
use tokio::runtime::{Builder, Runtime};
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

/// Spawn a future on core's background runtime, independent of any ambient
/// tokio runtime at the call site. The runtime lives for the process lifetime.
pub fn spawn_background<F>(fut: F) -> JoinHandle<F::Output>
where
    F: Future + Send + 'static,
    F::Output: Send + 'static,
{
    runtime().spawn(fut)
}
