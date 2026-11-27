use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Mutex;

use tauri::Manager;

static LIGHTWEIGHT_MODE: AtomicBool = AtomicBool::new(false);
/// 进出轻量模式与第二实例唤起共用，避免一边 destroy 主窗口、另一边重建。
static LIFECYCLE_LOCK: Mutex<()> = Mutex::new(());

fn lifecycle_lock() -> std::sync::MutexGuard<'static, ()> {
    LIFECYCLE_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// 持有轻量模式生命周期锁执行闭包（供第二实例回调等外部临界区使用）。
pub fn with_lifecycle_lock<R>(f: impl FnOnce() -> R) -> R {
    let _guard = lifecycle_lock();
    f()
}

pub fn enter_lightweight_mode(app: &tauri::AppHandle) -> Result<(), String> {
    let _guard = lifecycle_lock();
    enter_lightweight_mode_unlocked(app)
}

pub fn exit_lightweight_mode(app: &tauri::AppHandle) -> Result<(), String> {
    let _guard = lifecycle_lock();
    exit_lightweight_mode_unlocked(app)
}

/// 调用方已持有 [`with_lifecycle_lock`] 时使用，避免重入死锁。
pub(crate) fn enter_lightweight_mode_unlocked(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        crate::save_window_state_before_exit(app);
        window
            .destroy()
            .map_err(|e| format!("销毁主窗口失败: {e}"))?;
    }
    // else: already in lightweight mode or window not found, just set the flag

    #[cfg(target_os = "macos")]
    crate::tray::apply_tray_policy(app, false);

    LIGHTWEIGHT_MODE.store(true, Ordering::Release);
    #[cfg(feature = "session-monitor")]
    crate::session_manager::cache::global().set_enabled(false);
    persist_lightweight_mode(true);
    // 托盘面板也是一个 WebView：收着的当场释放，开着的（在面板里点的轻量模式）收起时释放。
    crate::tray_panel::release_if_lightweight(app);
    crate::tray::refresh_tray_menu(app);
    if let Some(state) = app.try_state::<crate::store::AppState>() {
        let _ = state.db.shrink_memory();
    }
    crate::trim_process_memory();
    log::info!("进入轻量模式");
    Ok(())
}

/// 按 `tauri.conf.json` 的 `"main"` 配置按需创建主窗口（已存在则直接返回）。
pub(crate) fn create_main_window(app: &tauri::AppHandle) -> Result<tauri::WebviewWindow, String> {
    if let Some(window) = app.get_webview_window("main") {
        return Ok(window);
    }
    let window_config = app
        .config()
        .app
        .windows
        .iter()
        .find(|w| w.label == "main")
        .ok_or("主窗口配置未找到")?;

    tauri::WebviewWindowBuilder::from_config(app, window_config)
        .map_err(|e| format!("加载主窗口配置失败: {e}"))?
        .build()
        .map_err(|e| format!("创建主窗口失败: {e}"))
}

/// 调用方已持有 [`with_lifecycle_lock`] 时使用，避免重入死锁。
pub(crate) fn exit_lightweight_mode_unlocked(app: &tauri::AppHandle) -> Result<(), String> {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.unminimize();
        let _ = window.show();
        let _ = window.set_focus();
        #[cfg(target_os = "linux")]
        {
            crate::linux_fix::nudge_main_window(window.clone(), "lightweight-exit");
        }
        #[cfg(target_os = "windows")]
        {
            let _ = window.set_skip_taskbar(false);
        }
        #[cfg(target_os = "macos")]
        {
            crate::tray::apply_tray_policy(app, true);
        }
        LIGHTWEIGHT_MODE.store(false, Ordering::Release);
        #[cfg(feature = "session-monitor")]
        crate::session_manager::cache::global().set_enabled(true);
        persist_lightweight_mode(false);
        crate::tray::refresh_tray_menu(app);
        crate::startup::trigger_session_sync_if_needed(app);
        log::info!("退出轻量模式");
        return Ok(());
    }

    let window = create_main_window(app)?;
    // 重建时 window-state 插件同样会恢复旧位置
    #[cfg(target_os = "windows")]
    {
        let _ = window.set_decorations(false);
        let _ = window.set_shadow(true);
    }
    #[cfg(target_os = "linux")]
    let _ = window.set_decorations(!crate::settings::get_settings().use_app_window_controls);
    crate::window_fit::fit_to_work_area(&window);
    let _ = window.unminimize();
    let _ = window.show();
    let _ = window.set_focus();
    #[cfg(target_os = "linux")]
    {
        crate::linux_fix::nudge_main_window(window.clone(), "lightweight-recreated");
    }

    #[cfg(target_os = "windows")]
    {
        let _ = window.set_skip_taskbar(false);
    }
    #[cfg(target_os = "macos")]
    {
        crate::tray::apply_tray_policy(app, true);
    }

    LIGHTWEIGHT_MODE.store(false, Ordering::Release);
    #[cfg(feature = "session-monitor")]
    crate::session_manager::cache::global().set_enabled(true);
    persist_lightweight_mode(false);
    crate::tray::refresh_tray_menu(app);
    crate::startup::trigger_session_sync_if_needed(app);
    log::info!("退出轻量模式");
    Ok(())
}

/// 记进 settings.json，下次启动按它恢复；写失败只记日志，不影响这次切换。
fn persist_lightweight_mode(enabled: bool) {
    if let Err(e) = crate::settings::set_lightweight_mode(enabled) {
        log::warn!("保存轻量模式状态失败: {e}");
    }
}

pub fn is_lightweight_mode() -> bool {
    LIGHTWEIGHT_MODE.load(Ordering::Acquire)
}
