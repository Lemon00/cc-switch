//! 可独立测试的启动完成信号、退出协调和启动窗口策略。
use std::future::Future;
use tokio::sync::{watch, OnceCell};

pub(crate) struct StartupGate(watch::Sender<bool>);

impl Default for StartupGate {
    fn default() -> Self {
        Self(watch::channel(false).0)
    }
}

impl StartupGate {
    pub fn is_ready(&self) -> bool {
        *self.0.borrow()
    }

    pub fn complete(&self) {
        self.0.send_replace(true);
    }

    pub async fn wait(&self) {
        let mut ready = self.0.subscribe();
        let _ = ready.wait_for(|ready| *ready).await;
    }
}

#[derive(Default)]
pub(crate) struct ExitCleanup(OnceCell<()>);

impl ExitCleanup {
    /// 并发调用等待同一轮；取消或 panic 不会把协调器永久留在 Running。
    pub async fn run(&self, cleanup: impl Future<Output = ()>) {
        self.0.get_or_init(|| cleanup).await;
    }
}

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StartupVisibility {
    Show,
    Silent,
    Lightweight,
}

pub(crate) fn startup_visibility(
    lightweight: bool,
    silent: bool,
    interactive: bool,
) -> StartupVisibility {
    if interactive {
        StartupVisibility::Show
    } else if lightweight {
        StartupVisibility::Lightweight
    } else if silent {
        StartupVisibility::Silent
    } else {
        StartupVisibility::Show
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    use std::time::Duration;
    use tokio::sync::oneshot;

    #[tokio::test]
    async fn startup_waits_until_complete_and_remembers_completion() {
        let gate = Arc::new(StartupGate::default());
        let waiter = {
            let gate = gate.clone();
            tokio::spawn(async move { gate.wait().await })
        };
        tokio::task::yield_now().await;
        assert!(!gate.is_ready());
        assert!(!waiter.is_finished());
        gate.complete();
        tokio::time::timeout(Duration::from_secs(1), waiter)
            .await
            .unwrap()
            .unwrap();
        assert!(gate.is_ready());
        tokio::time::timeout(Duration::from_secs(1), gate.wait())
            .await
            .unwrap();
    }

    #[tokio::test]
    async fn cleanup_callers_wait_for_one_completed_run() {
        let cleanup = Arc::new(ExitCleanup::default());
        let calls = Arc::new(AtomicUsize::new(0));
        let (started, start) = oneshot::channel();
        let (release, released) = oneshot::channel();
        let first = {
            let cleanup = cleanup.clone();
            let calls = calls.clone();
            tokio::spawn(async move {
                cleanup
                    .run(async {
                        calls.fetch_add(1, Ordering::SeqCst);
                        started.send(()).unwrap();
                        released.await.unwrap();
                    })
                    .await;
            })
        };
        start.await.unwrap();
        let second = {
            let cleanup = cleanup.clone();
            let calls = calls.clone();
            tokio::spawn(async move {
                cleanup
                    .run(async {
                        calls.fetch_add(1, Ordering::SeqCst);
                    })
                    .await
            })
        };
        tokio::task::yield_now().await;
        assert!(!second.is_finished());
        release.send(()).unwrap();
        first.await.unwrap();
        tokio::time::timeout(Duration::from_secs(1), second)
            .await
            .unwrap()
            .unwrap();
        cleanup
            .run(async {
                calls.fetch_add(1, Ordering::SeqCst);
            })
            .await;
        assert_eq!(calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn cancelled_cleanup_can_be_retried() {
        let cleanup = Arc::new(ExitCleanup::default());
        let (started, start) = oneshot::channel();
        let task = {
            let cleanup = cleanup.clone();
            tokio::spawn(async move {
                cleanup
                    .run(async {
                        started.send(()).unwrap();
                        std::future::pending::<()>().await;
                    })
                    .await;
            })
        };
        start.await.unwrap();
        task.abort();
        assert!(task.await.unwrap_err().is_cancelled());
        let mut retried = false;
        tokio::time::timeout(
            Duration::from_secs(1),
            cleanup.run(async {
                retried = true;
            }),
        )
        .await
        .unwrap();
        assert!(retried);
    }

    #[tokio::test]
    async fn panicking_cleanup_can_be_retried() {
        let cleanup = Arc::new(ExitCleanup::default());
        let first = {
            let cleanup = cleanup.clone();
            tokio::spawn(async move {
                cleanup.run(async { panic!("fixture cleanup panic") }).await;
            })
        };
        assert!(first.await.unwrap_err().is_panic());
        let mut retried = false;
        tokio::time::timeout(
            Duration::from_secs(1),
            cleanup.run(async { retried = true }),
        )
        .await
        .unwrap();
        assert!(retried);
    }

    #[test]
    fn interactive_launch_overrides_hidden_preferences() {
        for lightweight in [false, true] {
            for silent in [false, true] {
                assert_eq!(
                    startup_visibility(lightweight, silent, true),
                    StartupVisibility::Show
                );
            }
        }
        assert_eq!(
            startup_visibility(true, true, false),
            StartupVisibility::Lightweight
        );
        assert_eq!(
            startup_visibility(false, true, false),
            StartupVisibility::Silent
        );
        assert_eq!(
            startup_visibility(false, false, false),
            StartupVisibility::Show
        );
    }
}
