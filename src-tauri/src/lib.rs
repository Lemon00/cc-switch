pub use cc_switch_core::agy_config;
#[cfg(all(test, unix, feature = "session-monitor"))]
mod agy_smoke_tests;
pub use cc_switch_core::app_config;
mod app_store;
mod auto_launch;

pub use cc_switch_core::claude_mcp;
mod claude_plugin;
pub use cc_switch_core::codex_config;
mod codex_history_migration;
pub use cc_switch_core::codex_rollout_file;
pub use cc_switch_core::codex_state_db;
mod commands;
mod frontend_hooks;
pub use cc_switch_core::config;
pub use cc_switch_core::database;
mod deeplink;
pub use cc_switch_core::dsh_config;
pub use cc_switch_core::error;

mod init_status;
pub use cc_switch_core::jsonc_document;

mod lifecycle;
mod lightweight;
#[cfg(target_os = "linux")]
mod linux_fix;
pub use cc_switch_core::live;

pub use cc_switch_core::mcp;
pub use cc_switch_core::mode;
pub use cc_switch_core::model_capabilities;

pub use cc_switch_core::opencode_config;
mod panic_hook;

pub use cc_switch_core::prompt;
pub use cc_switch_core::prompt_files;
pub use cc_switch_core::provider;
pub use cc_switch_core::proxy;
mod services;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::session_manager;
pub use cc_switch_core::settings;
pub use cc_switch_core::store;
mod startup;
mod sync_utils;

mod tray;
#[cfg(target_os = "windows")]
mod tray_compat;
mod tray_panel;
pub use cc_switch_core::usage_events;
pub use cc_switch_core::usage_script;
mod window_fit;

pub use app_config::{AppType, InstalledSkill, McpApps, McpServer, MultiAppConfig, SkillApps};
pub use codex_config::{
    extract_codex_experimental_bearer_token, get_codex_auth_path, get_codex_config_path,
    read_codex_live_settings, write_codex_live_atomic,
};
pub use commands::open_provider_terminal;
pub use commands::*;
pub use config::{get_claude_mcp_path, get_claude_settings_path, read_json_file};
pub use database::{Database, Profile};
pub use deeplink::{import_provider_from_deeplink, parse_deeplink_url, DeepLinkImportRequest};
pub use error::AppError;
pub use mcp::{
    import_from_claude, import_from_codex, remove_server_from_claude, remove_server_from_codex,
    sync_single_server_to_claude, sync_single_server_to_codex,
};
pub use prompt::Prompt;
pub use provider::{Provider, ProviderMeta};
#[doc(hidden)]
pub use services::provider::claude_direct::settle_claude_tombstones;
pub use services::{
    profile::{ProfilePayload, ProfileScope, ProfileService, NO_ACTIVE_PROMPT},
    provider::{
        reapply_current_codex_official_live, reapply_current_codex_third_party_live, EditorSave,
        EditorView,
    },
    skill::{migrate_skills_to_ssot, ImportSkillSelection},
    ConfigService, EndpointLatency, McpService, PromptService, ProviderService, ProxyService,
    SkillService, SpeedtestService,
};
pub use settings::{update_settings, AppSettings};

pub use store::AppState;
use tauri_plugin_deep_link::DeepLinkExt;
use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};

use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, OnceLock};
use tauri::tray::TrayIconBuilder;
use tauri::RunEvent;
use tauri::{Emitter, Manager};
use tauri_plugin_window_state::{AppHandleExt, StateFlags};

#[cfg(target_os = "windows")]
fn set_windows_app_user_model_id(app: &tauri::AppHandle) {
    let app_id = app.config().identifier.clone();
    let wide_app_id: Vec<u16> = app_id.encode_utf16().chain(std::iter::once(0)).collect();

    let result = unsafe {
        windows_sys::Win32::UI::Shell::SetCurrentProcessExplicitAppUserModelID(wide_app_id.as_ptr())
    };

    if result < 0 {
        log::warn!("设置 Windows AppUserModelID 失败: 0x{result:08X}");
    } else {
        log::debug!("Windows AppUserModelID 已设置为 {app_id}");
    }
}

#[cfg(target_os = "windows")]
fn disable_windows_general_autofill<R: tauri::Runtime>(webview: &tauri::Webview<R>) {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2Settings4;
    use windows::core::Interface;

    let result = webview.with_webview(|platform_webview| {
        let result = unsafe {
            platform_webview
                .controller()
                .CoreWebView2()
                .and_then(|webview| webview.Settings())
                .and_then(|settings| settings.cast::<ICoreWebView2Settings4>())
                .and_then(|settings| settings.SetIsGeneralAutofillEnabled(false))
        };

        if let Err(error) = result {
            log::warn!("禁用 Windows WebView2 常规自动填充失败: {error}");
        }
    });

    if let Err(error) = result {
        log::warn!("访问 Windows WebView2 失败: {error}");
    }
}

// 脱敏这一整套（RedactedUrl / url_for_log / redact_* / error_for_log）已在核心实现，
// 这里只重导出，避免 shim 与 core 两份实现各自漂移。
// 保留完整重导出面（GUI 现在只用到 url_for_log），故允许少量未使用项。
#[allow(unused_imports)]
pub(crate) use cc_switch_core::{
    error_for_log, redact_known_secrets, redact_known_secrets_strict, redact_url_for_log,
    redact_url_for_log_with_secrets, redact_url_origin_for_log, url_for_log,
    url_for_log_with_secrets, RedactedUrl,
};

fn runtime_log_level_allows(level: log::Level, max_level: log::LevelFilter) -> bool {
    max_level.to_level().is_some_and(|maximum| level <= maximum)
}

/// 统一处理 ccswitch:// 深链接 URL
///
/// - 解析 URL，把结果（或错误）暂存为待处理深链接，再发 `deeplink-pending` 通知前端来读
///   （前端可能还没挂载，见 `deeplink::PendingDeepLink`）
/// - 可选：聚焦主窗口
fn handle_deeplink_url(
    app: &tauri::AppHandle,
    url_str: &str,
    focus_main_window: bool,
    source: &str,
) -> bool {
    if !url_str.starts_with("ccswitch://") {
        return false;
    }

    log::info!(
        "✓ Deep link URL detected from {source}: {}",
        url_for_log(url_str)
    );

    let pending = match crate::deeplink::parse_supported_deeplink_url(url_str) {
        Ok(request) => {
            log::info!(
                "✓ Successfully parsed deep link: resource={}, app={:?}, name={:?}",
                request.resource,
                request.app,
                request.name
            );
            crate::deeplink::PendingDeepLink::Import {
                request: Box::new(request),
            }
        }
        Err(e) => {
            log::error!("✗ Failed to parse deep link URL: {e}");
            crate::deeplink::PendingDeepLink::Error {
                url: "ccswitch://v1/import".to_string(),
                error: e.to_string(),
            }
        }
    };
    crate::deeplink::set_pending_deeplink(pending);

    if let Err(e) = app.emit("deeplink-pending", ()) {
        log::error!("✗ Failed to emit deeplink-pending event: {e}");
    }

    if focus_main_window {
        tray::show_main_window(app);
    }

    true
}

/// 更新托盘菜单的Tauri命令
#[tauri::command]
async fn update_tray_menu(
    app: tauri::AppHandle,
    state: tauri::State<'_, AppState>,
) -> Result<bool, String> {
    if !tray_panel::uses_native_menu() {
        // 用托盘面板、不挂原生菜单：更新图标状态，通知开着的面板重新取数据。
        tray::refresh_tray_menu(&app);
        return Ok(app.tray_by_id(tray::TRAY_ID).is_some());
    }
    match tray::create_tray_menu(&app, state.inner()) {
        Ok(new_menu) => {
            if let Some(tray) = app.tray_by_id(tray::TRAY_ID) {
                tray.set_menu(Some(new_menu))
                    .map_err(|e| format!("更新托盘菜单失败: {e}"))?;
                return Ok(true);
            }
            Ok(false)
        }
        Err(err) => {
            log::error!("创建托盘菜单失败: {err}");
            Err(format!("创建托盘菜单失败: {err}"))
        }
    }
}

/// 本进程打开文件描述符的软上限目标。macOS 的 `setrlimit` 不接受超过 `OPEN_MAX`（10240）的值。
#[cfg(unix)]
const OPEN_FILE_SOFT_LIMIT_TARGET: libc::rlim_t = 10240;

/// 从 Finder / 登录项启动的 macOS 应用继承 launchd 的软上限，默认只有 256 个文件描述符。
/// 本地路由并发时客户端连接、上游连接、日志、数据库和配置文件都要占用描述符，突发请求下
/// accept、上游连接和打开数据库会一起报 EMFILE（Too many open files）。启动时把软上限
/// 提到硬上限（封顶 10240），硬上限更低或已经足够时不动。
#[cfg(unix)]
fn raise_open_file_limit() {
    let mut limit = libc::rlimit {
        rlim_cur: 0,
        rlim_max: 0,
    };
    // SAFETY: 传入的是有效的 rlimit 指针，调用只读写这个结构体。
    if unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) } != 0 {
        log::warn!(
            "读取文件描述符上限失败: {}",
            std::io::Error::last_os_error()
        );
        return;
    }
    let target = OPEN_FILE_SOFT_LIMIT_TARGET.min(limit.rlim_max);
    if limit.rlim_cur >= target {
        return;
    }
    let previous = limit.rlim_cur;
    limit.rlim_cur = target;
    // SAFETY: 同上；软上限不超过硬上限，不需要特权。
    if unsafe { libc::setrlimit(libc::RLIMIT_NOFILE, &limit) } != 0 {
        log::warn!(
            "提高文件描述符上限失败（当前 {previous}）: {}",
            std::io::Error::last_os_error()
        );
    } else {
        log::info!("文件描述符软上限 {previous} → {target}");
    }
}

/// 又启动了一个实例（或经命令行打开 ccswitch:// 链接）：退出轻量模式、处理链接、把主窗口拿到前台。
/// 在后台线程上调用（见 single_instance 回调），一次处理一个：连着启动两次时，
/// 不能两边都看到主窗口不在、各建一个。
#[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
fn handle_second_instance(app: &tauri::AppHandle, args: Vec<String>) {
    startup::request_main_window();
    // 本函数在后台线程运行；启动收尾不能在显式打开之后再隐藏/销毁窗口。
    tauri::async_runtime::block_on(crate::init_status::wait_for_startup());
    if USER_EXIT_IN_PROGRESS.load(Ordering::SeqCst) {
        return;
    }
    // 与 enter/exit_lightweight_mode 共用同一把锁，避免一边销毁主窗口、
    // 另一边按 label 重建时交错。
    crate::lightweight::with_lifecycle_lock(|| {
        log::debug!("Args count: {}", args.len());
        for (i, arg) in args.iter().enumerate() {
            log::debug!("  arg[{i}]: {}", url_for_log(arg));
        }

        if crate::lightweight::is_lightweight_mode() {
            if let Err(e) = crate::lightweight::exit_lightweight_mode_unlocked(app) {
                log::error!("退出轻量模式重建窗口失败: {e}");
            }
        }

        // Check for deep link URL in args (mainly for Windows/Linux command line)
        let mut found_deeplink = false;
        for arg in &args {
            if handle_deeplink_url(app, arg, false, "single_instance args") {
                found_deeplink = true;
                break;
            }
        }

        if !found_deeplink {
            log::info!(
                "ℹ No deep link URL found in args (this is expected on macOS when launched via system)"
            );
        }

        // Show and focus window regardless
        if let Some(window) = app.get_webview_window("main") {
            // 防御性重置 Windows 的 skip_taskbar：single_instance 触发时，
            // 原进程可能因 silent_startup / 关闭到托盘等处于 skip_taskbar(true) 状态，
            // 仅 show() 不会重置该状态，会导致窗口可见但不在任务栏、最小化后消失。
            #[cfg(target_os = "windows")]
            {
                let _ = window.set_skip_taskbar(false);
            }
            let _ = window.unminimize();
            let _ = window.show();
            let _ = window.set_focus();
            #[cfg(target_os = "linux")]
            {
                linux_fix::nudge_main_window(window.clone(), "single-instance");
            }
        }
    });
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
/// 尝试把 Rust/SQLite 释放后仍留在 glibc arena 中的空闲页还给操作系统。
pub(crate) fn trim_process_memory() {
    #[cfg(all(target_os = "linux", target_env = "gnu"))]
    // SAFETY: malloc_trim(0) 是 glibc 线程安全的无状态堆整理接口。
    unsafe {
        libc::malloc_trim(0);
    }
}

pub fn run() {
    // Register before any plugin or startup task touches Tauri's global runtime.
    tauri::async_runtime::set(cc_switch_core::background::runtime_handle());

    // 设置 panic hook，在应用崩溃时记录日志到 <app_config_dir>/crash.log（默认 ~/.cc-switch/crash.log）
    panic_hook::setup_panic_hook();

    let mut builder = tauri::Builder::default();

    // WebView settings are per-view: apply this to initial and recreated windows,
    // including the lightweight tray panel (#9216), before they are shown.
    #[cfg(target_os = "linux")]
    {
        builder = builder.plugin(
            tauri::plugin::Builder::<_, ()>::new("linux-webview-rendering")
                .on_webview_ready(|webview| linux_fix::configure_webview(&webview))
                .build(),
        );
    }

    #[cfg(target_os = "windows")]
    {
        builder = builder.plugin(
            tauri::plugin::Builder::<_, ()>::new("windows-general-autofill")
                .on_webview_ready(|webview| {
                    if webview.label() == "main" {
                        disable_windows_general_autofill(&webview);
                    }
                })
                .build(),
        );
    }

    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    {
        builder = builder.plugin(tauri_plugin_single_instance::init(|app, args, _cwd| {
            log::info!("=== Single Instance Callback Triggered ===");
            // Windows 上这个回调跑在主线程处理 WM_COPYDATA 的窗口过程里，第二个实例的
            // SendMessage 正等它返回：轻量模式下要在这里同步重建主窗口，WebView2 会把主线程
            // 卡死（托盘随之无响应）。挪到后台线程，建窗口改由事件循环处理，和 macOS / Linux
            // 本来就不在主线程上调用一致。
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || handle_second_instance(&app, args));
        }));
    }

    #[cfg(target_os = "windows")]
    {
        let startup_page_handled = AtomicBool::new(false);
        builder = builder.on_page_load(move |webview, payload| {
            if webview.label() == "main"
                && payload.event() == tauri::webview::PageLoadEvent::Finished
                && payload.url().scheme() != "about"
                && !startup_page_handled.swap(true, Ordering::Relaxed)
                && !crate::settings::get_settings().silent_startup
                && !crate::settings::get_settings().lightweight_mode
            {
                let window = webview.window();
                if window.show().is_ok() {
                    let _ = window.emit("main-window-visibility-changed", ());
                }
                log::info!("主页面加载完成，主窗口已显示");
            }
        });
    }

    let builder = builder
        // 注册 deep-link 插件（处理 macOS AppleEvent 和其他平台的深链接）
        .plugin(tauri_plugin_deep_link::init())
        // 拦截窗口关闭：根据设置决定是否最小化到托盘
        .on_window_event(|window, event| {
            // 托盘面板自己处理关闭（只隐藏），不走主窗口的「最小化到托盘 / 退出」
            if window.label() == tray_panel::PANEL_LABEL {
                return;
            }
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                log::info!("收到窗口关闭请求: label={}", window.label());
                // 数据库版本过新的恢复模式下没有托盘可唤回，关闭即退出，避免应用隐身后台
                let in_db_recovery = crate::init_status::get_init_error().is_some()
                    || !crate::init_status::startup_ready();
                if in_db_recovery {
                    api.prevent_close();
                    window.app_handle().exit(0);
                    return;
                }

                let settings = crate::settings::get_settings();

                if settings.minimize_to_tray_on_close {
                    api.prevent_close();
                    if window.hide().is_ok() {
                        // An already-unfocused window may emit no blur/resize.
                        let _ = window.emit("main-window-visibility-changed", ());
                    }
                    log::info!("关闭请求已处理：最小化到托盘");
                    #[cfg(target_os = "windows")]
                    {
                        let _ = window.set_skip_taskbar(true);
                    }
                    #[cfg(target_os = "macos")]
                    {
                        tray::apply_tray_policy(window.app_handle(), false);
                    }
                    if let Some(state) = window.app_handle().try_state::<store::AppState>() {
                        let _ = state.db.shrink_memory();
                    }
                    trim_process_memory();
                } else {
                    api.prevent_close();
                    log::info!("关闭请求已处理：退出应用");
                    window.app_handle().exit(0);
                }
            }
        })
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_store::Builder::new().build())
        .plugin(
            tauri_plugin_window_state::Builder::default()
                .with_state_flags(window_state_flags())
                .with_denylist(&[tray_panel::PANEL_LABEL])
                .build(),
        )
        .setup(|app| {
            let _ = rustls::crypto::ring::default_provider().install_default();

            // 预先决定配置目录覆盖（env > Store > 默认），确保后续路径读取正确
            // （日志/数据库等）。必须在 Database::init 之前。
            let _ = app_store::refresh_app_config_dir_override(app.handle());
            panic_hook::init_app_config_dir(crate::config::get_app_config_dir());

            // 初始化日志（输出到 <app_config_dir>/logs/cc-switch.log）
            {
                use tauri_plugin_log::{RotationStrategy, Target, TargetKind, TimezoneStrategy};

                let log_dir = panic_hook::get_log_dir();

                // 确保日志目录存在
                if let Err(e) = std::fs::create_dir_all(&log_dir) {
                    eprintln!("创建日志目录失败: {e}");
                }

                app.handle().plugin(
                    tauri_plugin_log::Builder::default()
                        // 底层保留 Trace 能力，便于加载用户配置后动态调高级别。
                        // 插件注册后会立即把全局级别收紧到 Info，避免启动阶段全量 Trace。
                        .level(log::LevelFilter::Trace)
                        // plugin-log 的前端 command 会直达 logger，绕过 log 宏的全局
                        // max_level；在分发层补一次过滤，确保动态总开关同样约束前端日志。
                        .filter(|metadata| {
                            runtime_log_level_allows(metadata.level(), log::max_level())
                        })
                        .targets([
                            Target::new(TargetKind::Stdout),
                            Target::new(TargetKind::Folder {
                                path: log_dir,
                                file_name: Some("cc-switch".into()),
                            }),
                        ])
                        // KeepSome(4) 保留 4 个轮转归档，加上当前文件最多约 100 MiB。
                        // 轮转仅按大小触发；跨重启继续追加，不再丢失上一次运行的日志。
                        .rotation_strategy(RotationStrategy::KeepSome(4))
                        .max_file_size(20 * 1024 * 1024)
                        .timezone_strategy(TimezoneStrategy::UseLocal)
                        .build(),
                )?;

                // 用户配置存在数据库中，数据库尚未打开时使用保守的 Info 级别。
                log::set_max_level(log::LevelFilter::Info);
                log::info!("=== CC Switch v{} started ===", env!("CARGO_PKG_VERSION"));
            }

            #[cfg(unix)]
            raise_open_file_limit();

            // 首次读取覆盖路径时 logger 尚未可用；此处重放一次，
            // 让 Store 损坏或路径无效等启动警告能够真正落盘。
            let _ = app_store::refresh_app_config_dir_override(app.handle());

            #[cfg(target_os = "windows")]
            set_windows_app_user_model_id(app.handle());

            // 注册 Updater 插件（桌面端）；放在 logger 之后，确保失败可诊断。
            #[cfg(desktop)]
            {
                let updater_status = app
                    .handle()
                    .plugin(tauri_plugin_updater::Builder::new().build())
                    .map_err(|error| format!("初始化 Updater 插件失败: {error}"));
                if let Err(error) = &updater_status {
                    // 若配置不完整（如缺少 pubkey），跳过 Updater 而不中断应用
                    log::warn!("{error}");
                }
                app.manage(crate::init_status::UpdaterStatus(updater_status));
            }

            // 注入 AppHandle 给 usage_events，让无 AppHandle 持有的写日志路径
            // 也能向前端推送 `usage-log-recorded`。
            // 放在日志系统初始化之后，确保 init 的日志能正常输出。
            usage_events::init(std::sync::Arc::new(frontend_hooks::TauriEventSink(
                app.handle().clone(),
            )));

            // 轻量模式下启动时不创建主窗口（减少 WebView2 / WebKitGTK 进程创建），
            // 仅在非轻量模式下预先建立隐藏主窗口；异常或深链接唤起时再按需创建。
            let settings = crate::settings::get_settings();
            if !settings.lightweight_mode {
                match crate::lightweight::create_main_window(app.handle()) {
                    Ok(window) => {
                        #[cfg(not(target_os = "windows"))]
                        if !settings.silent_startup {
                            let _ = window.show();
                        }
                        #[cfg(target_os = "windows")]
                        let _ = window;
                    }
                    Err(error) => {
                        log::error!("启动创建主窗口失败: {error}");
                    }
                }
            }
            startup::start(app.handle().clone());

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_providers,
            commands::refresh_providers,
            commands::get_unreadable_providers,
            commands::get_current_provider,
            commands::add_provider,
            commands::update_provider,
            commands::get_provider_editor_view,
            commands::delete_provider,
            commands::remove_provider_from_live_config,
            commands::switch_provider,
            commands::import_default_config,
            commands::ensure_codex_official_provider,
            commands::get_claude_config_status,
            commands::get_config_status,
            commands::get_claude_code_config_path,
            commands::get_config_dir,
            commands::open_config_folder,
            commands::pick_directory,
            commands::open_external,
            commands::get_init_error,
            commands::get_migration_result,
            commands::get_skills_migration_result,
            commands::get_app_config_path,
            commands::open_app_config_folder,
            commands::reveal_app_log_file,
            commands::get_claude_common_config_snippet,
            commands::set_claude_common_config_snippet,
            commands::get_common_config_snippet,
            commands::set_common_config_snippet,
            commands::extract_common_config_snippet,
            commands::read_live_provider_settings,
            commands::get_settings,
            commands::save_settings,
            commands::has_codex_unify_history_backup,
            commands::codex_forces_multi_agent_v2,
            commands::get_codex_session_compression,
            commands::set_codex_session_compression,
            #[cfg(feature = "session-monitor")]
            commands::get_codex_sessions_disk_usage,
            commands::restore_codex_unified_history,
            commands::get_rectifier_config,
            commands::set_rectifier_config,
            commands::get_optimizer_config,
            commands::set_optimizer_config,
            commands::get_copilot_optimizer_config,
            commands::set_copilot_optimizer_config,
            commands::get_log_config,
            commands::set_log_config,
            commands::restart_app,
            commands::install_update_and_restart,
            commands::check_app_update_available,
            commands::check_for_updates,
            commands::open_portable_update_download,
            commands::is_portable_mode,
            commands::copy_text_to_clipboard,
            commands::get_claude_plugin_status,
            commands::read_claude_plugin_config,
            commands::apply_claude_plugin_config,
            commands::is_claude_plugin_applied,
            commands::apply_claude_onboarding_skip,
            commands::clear_claude_onboarding_skip,
            // Claude MCP management
            commands::get_claude_mcp_status,
            commands::read_claude_mcp_config,
            commands::upsert_claude_mcp_server,
            commands::delete_claude_mcp_server,
            commands::validate_mcp_command,
            // usage query
            commands::queryProviderUsage,
            commands::testUsageScript,
            // subscription quota
            commands::get_subscription_quota,
            commands::get_codex_oauth_quota,
            commands::get_codex_oauth_models,
            commands::get_xai_oauth_models,
            commands::get_xai_oauth_quota,
            commands::get_coding_plan_quota,
            commands::get_balance,
            // New MCP via config.json (SSOT)
            commands::get_mcp_config,
            commands::upsert_mcp_server_in_config,
            commands::delete_mcp_server_in_config,
            commands::set_mcp_enabled,
            // Unified MCP management
            commands::get_mcp_servers,
            commands::get_unreadable_mcp_servers,
            commands::start_mcp_connectivity_checks,
            commands::cancel_mcp_connectivity_checks,
            commands::upsert_mcp_server,
            commands::delete_mcp_server,
            commands::toggle_mcp_app,
            commands::import_mcp_from_apps,
            commands::resync_mcp_to_apps,
            // Prompt management
            commands::get_prompts,
            commands::upsert_prompt,
            commands::delete_prompt,
            commands::enable_prompt,
            commands::import_prompt_from_file,
            commands::get_current_prompt_file_content,
            commands::get_prompt_file_location,
            // Pi native provider and session views
            // Oh My Pi native provider and session views
            // Profile management (项目配置方案)
            commands::list_profiles,
            commands::create_profile,
            commands::update_profile,
            commands::delete_profile,
            commands::clear_current_profile,
            commands::apply_profile,
            // Fetch OpenAI-compatible and Anthropic model lists. Response data structure:
            // data[].id, data[]?.owned_by. Special: supports Zhipu OpenAI Responses models[].slug.
            commands::fetch_models_for_config,
            commands::get_agy_models,
            commands::get_opencode_models,
            // ours: endpoint speed test + custom endpoint management
            commands::test_api_endpoints,
            commands::get_custom_endpoints,
            commands::add_custom_endpoint,
            commands::remove_custom_endpoint,
            commands::update_endpoint_last_used,
            // app_config_dir override via Store
            commands::get_app_config_dir_override,
            commands::set_app_config_dir_override,
            // provider sort order management
            commands::update_providers_sort_order,
            // theirs: config import/export and dialogs
            commands::export_config_to_file,
            commands::import_config_from_file,
            commands::webdav_test_connection,
            commands::webdav_sync_upload,
            commands::webdav_sync_download,
            commands::webdav_sync_save_settings,
            commands::webdav_sync_clear_settings,
            commands::webdav_sync_fetch_remote_info,
            commands::s3_test_connection,
            commands::s3_sync_upload,
            commands::s3_sync_download,
            commands::s3_sync_save_settings,
            commands::s3_sync_fetch_remote_info,
            commands::save_file_dialog,
            commands::open_file_dialog,
            commands::open_zip_file_dialog,
            commands::create_db_backup,
            commands::list_db_backups,
            commands::list_backup_locations,
            commands::delete_backup_location,
            commands::reveal_backup_location,
            commands::restore_db_backup,
            commands::rename_db_backup,
            commands::delete_db_backup,
            commands::sync_current_providers_live,
            // Deep link import
            commands::parse_deeplink,
            commands::merge_deeplink_config,
            commands::fetch_deeplink_icon,
            commands::import_from_deeplink,
            commands::import_from_deeplink_unified,
            commands::peek_pending_deeplink,
            commands::ack_pending_deeplink,
            update_tray_menu,
            // Environment variable management
            commands::check_env_conflicts,
            commands::delete_env_vars,
            commands::restore_env_backup,
            // Skill management (v3.10.0+ unified)
            commands::get_installed_skills,
            commands::refresh_local_skill_metadata,
            commands::get_skill_backups,
            commands::delete_skill_backup,
            commands::install_skill_unified,
            commands::uninstall_skill_unified,
            commands::restore_skill_backup,
            commands::toggle_skill_app,
            commands::scan_unmanaged_skills,
            commands::import_skills_from_apps,
            commands::discover_available_skills,
            commands::discover_repo_skills,
            commands::check_skill_updates,
            commands::resync_skills_to_apps,
            commands::update_skill,
            commands::migrate_skill_storage,
            commands::get_cc_switch_skills_dir,
            commands::open_cc_switch_skills_dir,
            commands::open_skill_dir,
            commands::search_skills_sh,
            // Skill management (legacy API compatibility)
            commands::get_skills,
            commands::get_skills_for_app,
            commands::install_skill,
            commands::install_skill_for_app,
            commands::uninstall_skill,
            commands::uninstall_skill_for_app,
            commands::get_skill_repos,
            commands::add_skill_repo,
            commands::remove_skill_repo,
            commands::install_skills_from_zip,
            // Auto launch
            commands::set_auto_launch,
            commands::get_auto_launch_status,
            // Proxy server management
            commands::start_proxy_server,
            commands::stop_proxy_server,
            commands::stop_proxy_with_restore,
            commands::get_proxy_takeover_status,
            commands::set_proxy_takeover_for_app,
            commands::get_app_mode,
            commands::set_proxy_route,
            commands::take_startup_attach_failures,
            tray::take_tray_navigation,
            tray_panel::tray_panel_state,
            tray_panel::tray_panel_placement,
            tray_panel::tray_panel_layout,
            tray_panel::tray_panel_hide,
            tray_panel::tray_panel_action,
            tray::tray_app_page_seen,
            commands::exit_proxy_apps_in_mode,
            commands::get_direct_provider,
            commands::get_proxy_status,
            commands::get_proxy_config,
            commands::update_proxy_config,
            // Global & Per-App Config
            commands::get_global_proxy_config,
            commands::update_global_proxy_config,
            commands::get_proxy_client_host,
            commands::get_proxy_lan_token,
            commands::set_proxy_client_host,
            commands::get_proxy_config_for_app,
            commands::update_proxy_config_for_app,
            commands::get_pricing_model_source,
            commands::set_pricing_model_source,
            commands::is_proxy_running,
            commands::is_live_takeover_active,
            commands::switch_proxy_provider,
            commands::get_proxy_stack,
            commands::set_proxy_stack_member,
            commands::adopt_codex_stack_catalog,
            commands::restart_codex_app_server_daemon,
            commands::acknowledge_codex_stale_clients,
            // Proxy failover commands
            commands::get_provider_health,
            commands::reset_circuit_breaker,
            commands::get_circuit_breaker_config,
            commands::update_circuit_breaker_config,
            commands::get_circuit_breaker_stats,
            // Failover queue management
            commands::get_failover_queue,
            commands::get_available_providers_for_failover,
            commands::add_to_failover_queue,
            commands::remove_from_failover_queue,
            commands::get_auto_failover_enabled,
            commands::set_auto_failover_enabled,
            // Usage statistics
            commands::get_usage_summary,
            commands::get_session_usage_summary,
            commands::get_usage_summary_by_app,
            commands::get_usage_trends,
            commands::get_usage_first_date,
            commands::get_provider_stats,
            commands::get_model_stats,
            commands::get_request_logs,
            commands::get_request_detail,
            commands::get_model_pricing,
            commands::update_model_pricing,
            commands::update_model_pricing_batch,
            commands::delete_model_pricing,
            commands::get_models_dev_sync_config,
            commands::save_models_dev_sync_config,
            commands::record_models_dev_sync_result,
            commands::fetch_models_dev,
            commands::sync_models_dev_pricing,
            commands::check_provider_limits,
            // Session usage sync
            #[cfg(feature = "session-monitor")]
            commands::sync_session_usage,
            #[cfg(feature = "session-monitor")]
            commands::rebuild_codex_usage,
            #[cfg(feature = "session-monitor")]
            commands::get_session_usage_last_sync,
            #[cfg(feature = "session-monitor")]
            commands::get_usage_data_sources,
            commands::fetch_exchange_rates,
            // Stream health check
            commands::stream_check_provider,
            // Session manager
            #[cfg(feature = "session-monitor")]
            commands::list_sessions,
            #[cfg(feature = "session-monitor")]
            commands::get_session_messages,
            #[cfg(feature = "session-monitor")]
            commands::stream_session_messages,
            #[cfg(feature = "session-monitor")]
            commands::get_session_block_content,
            #[cfg(feature = "session-monitor")]
            commands::get_session_image,
            #[cfg(feature = "session-monitor")]
            commands::reveal_session_path,
            #[cfg(feature = "session-monitor")]
            commands::export_session_markdown,
            #[cfg(feature = "session-monitor")]
            commands::delete_session,
            #[cfg(feature = "session-monitor")]
            commands::delete_sessions,
            #[cfg(feature = "session-monitor")]
            commands::launch_session_terminal,
            commands::get_tool_versions,
            commands::run_tool_lifecycle_action,
            commands::prepare_tool_uninstall,
            commands::probe_tool_installations,
            commands::list_tool_installations,
            // Provider terminal
            commands::open_provider_terminal,
            // Universal Provider management
            commands::get_universal_providers,
            commands::get_universal_provider,
            commands::upsert_universal_provider,
            commands::delete_universal_provider,
            commands::sync_universal_provider,
            // Custom icon library
            commands::get_custom_icons,
            commands::add_custom_icon,
            commands::rename_custom_icon,
            commands::delete_custom_icon,
            // OpenCode specific
            commands::import_opencode_providers_from_live,
            commands::get_opencode_live_provider_ids,
            commands::get_opencode_default_model,
            commands::set_opencode_default_model,
            // OpenClaw specific
            // Hermes specific
            // Global upstream proxy
            commands::get_global_proxy_url,
            commands::set_global_proxy_url,
            commands::test_proxy_url,
            commands::get_upstream_proxy_status,
            commands::scan_local_proxies,
            // Window theme control
            commands::set_window_theme,
            // Generic managed auth commands
            commands::auth_start_login,
            commands::auth_poll_for_account,
            commands::auth_cancel_login,
            commands::auth_list_accounts,
            commands::auth_get_status,
            commands::auth_remove_account,
            commands::auth_set_default_account,
            commands::auth_logout,
            commands::agy_account_is_running,
            commands::agy_account_quota,
            commands::claude_account_submit_paste,
            commands::get_claude_account_quota,
            commands::get_claude_default_plan,
            commands::get_claude_login_switch,
            commands::dsh_account_status,
            commands::dsh_account_sign_out,
            commands::dsh_account_login_start,
            commands::dsh_account_login_poll,
            commands::dsh_account_login_cancel,
            // Copilot OAuth commands (multi-account support)
            commands::copilot_start_device_flow,
            commands::copilot_poll_for_auth,
            commands::copilot_poll_for_account,
            commands::copilot_list_accounts,
            commands::copilot_remove_account,
            commands::copilot_set_default_account,
            commands::copilot_get_auth_status,
            commands::copilot_logout,
            commands::copilot_is_authenticated,
            commands::copilot_get_token,
            commands::copilot_get_token_for_account,
            commands::copilot_get_models,
            commands::copilot_get_models_for_account,
            commands::copilot_get_usage,
            commands::copilot_get_usage_for_account,
            // OMO commands
            commands::read_omo_local_file,
            commands::get_current_omo_provider_id,
            commands::disable_current_omo,
            commands::read_omo_slim_local_file,
            commands::get_current_omo_slim_provider_id,
            commands::disable_current_omo_slim,
            // Workspace files (OpenClaw)
            // Daily memory files (OpenClaw workspace)
            // lightweight mode (for testing or low-resource environments)
            commands::enter_lightweight_mode,
            commands::exit_lightweight_mode,
            commands::is_lightweight_mode,
        ]);

    let app = builder
        .build(tauri::generate_context!())
        .expect("error while running tauri application");

    app.run(|app_handle, event| {
        // 处理退出请求（所有平台）
        if let RunEvent::ExitRequested { api, code, .. } = &event {
            match classify_exit_request(*code) {
                // code 为 None 表示运行时自动触发（如隐藏窗口的 WebView 被回收导致无存活窗口），
                // 此时应仅阻止退出、保持托盘后台运行。
                ExitRequestAction::StayInTray => {
                    log::info!("运行时触发退出请求（无存活窗口），阻止退出以保持托盘后台运行");
                    api.prevent_exit();
                    return;
                }
                // code 为 RESTART_EXIT_CODE：app.restart() 发起的重启（本应用自己的重启
                // 都走 restart_process，不经过这里，此分支只兜底）。
                // 这条路径上 prevent_exit() 会被 Tauri 忽略，事件循环必定退出，随后由
                // Tauri 在 RunEvent::Exit 后用新二进制 re-exec（macOS 会按更新后的
                // Info.plist 解析可执行名）。
                //
                // 绝不能复用下面的异步清理任务：该任务在 tokio 线程调 save_window_state，
                // 持有 window-state 插件锁的同时向主线程查询窗口几何；而主线程此刻正在
                // 退出事件循环，并在插件自带的 RunEvent::Exit 钩子里等待同一把锁——双方
                // 互等造成进程永久卡死（更新已安装但应用冻结、不再重启，见 #3998）。
                //
                // 重启路径交还 Tauri 默认流程即可：
                //   - 窗口状态：插件 Exit 钩子在主线程保存（同线程读取窗口几何，无死锁）
                //   - 托盘图标：Tauri 内部 cleanup_before_exit 清理，正常走 Drop
                //   - 代理/Live 配置：无需恢复，重启后新实例立即接管并恢复代理状态
                //   - 100ms 落盘等待：重启前的 DB 写入均为命令驱动、此刻已完成，
                //     与所有 Tauri 应用默认重启路径的行为一致，无需额外等待
                ExitRequestAction::DeferToTauriRestart => {
                    log::info!("收到重启请求 (code={code:?})，交由 Tauri 默认重启流程 re-exec");
                    RESTART_REQUESTED.store(true, Ordering::SeqCst);
                    return;
                }
                // 其它 Some(_)：用户主动调用 app.exit() 退出（如托盘菜单"退出"），
                // 此时执行清理后退出。
                ExitRequestAction::CleanupAndExit => {}
            }

            if USER_EXIT_IN_PROGRESS.swap(true, Ordering::SeqCst) {
                log::info!("退出流程已在进行，忽略重复的 ExitRequested");
                api.prevent_exit();
                return;
            }

            log::info!("收到用户主动退出请求 (code={code:?})，开始清理...");
            api.prevent_exit();

            let exit_code = code.unwrap_or(0);
            let app_handle = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                // 后台初始化可能还在写客户端文件，待其结束后再做最终恢复。
                crate::init_status::wait_for_startup().await;
                save_window_state_before_exit(&app_handle);
                run_exit_cleanup_once(&app_handle).await;
                // 先于 std::process::exit 显式移除托盘图标。
                // 进程直接退出时 Tauri 运行时不走正常 Drop 流程，
                // 不会向 Windows Shell 发送 NIM_DELETE，导致已退出的进程
                // 注册的图标仍残留在系统托盘（鼠标悬停 Shell 才会重绘发现进程已死）。
                remove_tray_icon_before_exit(&app_handle);
                // 等在途数据库任务放掉连接，代替固定 sleep（sleep 与落盘无关）。
                wait_for_db_quiescent(&app_handle).await;
                destroy_single_instance_lock(&app_handle);
                log::info!("清理完成，退出应用");

                // 使用 std::process::exit 避免再次触发 ExitRequested
                std::process::exit(exit_code);
            });
            return;
        }

        // macOS ⌘Q、Dock「退出」、注销关机走系统 terminate，不发 ExitRequested、只发
        // RunEvent::Exit，回调一返回进程就结束，只能在这里同步补做退出清理。重启也会走到
        // 这里，照上面 DeferToTauriRestart 的约定交还 Tauri 默认流程，不清理。
        if matches!(event, RunEvent::Exit) {
            if !RESTART_REQUESTED.load(Ordering::SeqCst) {
                cleanup_before_system_exit(app_handle);
            }
            return;
        }

        #[cfg(target_os = "macos")]
        {
            // macOS 在 Dock 图标被点击并重新激活应用时会触发 Reopen 事件，这里手动恢复主窗口。
            // RunEvent::Opened（ccswitch:// 链接）不在这里处理：deep-link 插件的 on_event 已把它
            // 转成 on_open_url 回调，这里再处理一遍会让同一条链接触发两次导入。
            if let RunEvent::Reopen { .. } = event {
                // show_main_window 统一在后台等待启动和生命周期锁。
                tray::show_main_window(app_handle);
            }
        }

        #[cfg(not(target_os = "macos"))]
        {
            let _ = (app_handle, event);
        }
    });
}

// ============================================================
// 应用退出清理
// ============================================================

static EXIT_CLEANUP_COORD: OnceLock<lifecycle::ExitCleanup> = OnceLock::new();
/// 用户退出路径（ExitRequested → process::exit）只允许一个异步任务驱动。
static USER_EXIT_IN_PROGRESS: AtomicBool = AtomicBool::new(false);

/// 串行、一次性执行退出清理；中断的清理可由下一位调用者重试。
pub async fn run_exit_cleanup_once(app_handle: &tauri::AppHandle) {
    // 包括更新/目录重启在内的每个入口都必须等初始化写入结束。
    crate::init_status::wait_for_startup().await;
    EXIT_CLEANUP_COORD
        .get_or_init(lifecycle::ExitCleanup::default)
        .run(cleanup_before_exit(app_handle))
        .await;
}

/// 应用退出前的清理工作
///
/// 把接上代理的客户端都指回直连（模式和代理路由保留，下次启动再接上），再停止代理。
/// 客户端不能一直指着代理：开机自启默认关闭，CC Switch 一关客户端就连不上了。
pub async fn cleanup_before_exit(app_handle: &tauri::AppHandle) {
    if let Some(state) = app_handle.try_state::<store::AppState>() {
        crate::mode::controller::detach_all(state.inner()).await;
        // 切换后几秒内退出：给开着的 Claude Code 会话写的空值墓碑现在删掉（刚写的最多等 3 秒），
        // 不留到下次启动
        let _ = tokio::task::spawn_blocking(
            crate::services::provider::claude_direct::finish_tombstones_before_exit,
        )
        .await;
        log::info!("退出清理完成：客户端已指回直连，代理已停止");
    }
}

/// 系统终止应用时最多等退出清理这么久：停代理服务自带 5 秒超时，指回直连只是写几个文件。
///
/// 删 Claude 空值墓碑前最多等 3 秒（`claude_direct::TOMBSTONE_MIN_VISIBLE`），但这 3 秒从
/// 切换写入墓碑那一刻算，指回直连、停代理花掉的时间也算在里面，所以不会再叠加 3 秒：最坏
/// 是两者取大（5 秒）再加写几个文件。只有真写了墓碑、而且是切换后 3 秒内退出才会等。
/// 超时也不弄坏文件：空值留着（Claude Code 读到等于没设），下次启动接着删。
const SYSTEM_EXIT_CLEANUP_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(8);

/// 系统直接终止应用时（macOS ⌘Q、Dock「退出」、注销关机）的退出清理。
///
/// 这条路没有 `ExitRequested` 可以 `prevent_exit()` 再异步清理，只能在主线程上等清理做完。
/// 清理放到异步运行时的线程上跑、主线程限时等：万一里面有步骤要等主线程，超时后照常退出，
/// 不会把进程卡住。
fn cleanup_before_system_exit(app_handle: &tauri::AppHandle) {
    log::info!("系统终止应用，开始退出清理...");
    let handle = app_handle.clone();
    USER_EXIT_IN_PROGRESS.store(true, Ordering::SeqCst);
    let task = tauri::async_runtime::spawn(async move {
        crate::init_status::wait_for_startup().await;
        run_exit_cleanup_once(&handle).await;
    });
    // timeout 要在运行时里构造（它取当前运行时的计时器），所以包一层 async。
    let finished = tauri::async_runtime::block_on(async move {
        tokio::time::timeout(SYSTEM_EXIT_CLEANUP_TIMEOUT, task).await
    });
    if finished.is_err() {
        log::warn!(
            "退出清理 {} 秒内没做完，直接退出",
            SYSTEM_EXIT_CLEANUP_TIMEOUT.as_secs()
        );
    }
}

/// 主动从系统托盘移除托盘图标。
///
/// `std::process::exit` 会绕过 Tauri 运行时，触发不了 `TrayIcon::drop()`，
/// 也就不会向 Windows Shell 发 `NIM_DELETE`。结果是进程退出后托盘里
/// 仍保留一个死图标的缓存占位（Shell 不会主动重绘，需要鼠标悬停才刷新）。
///
/// 通过 `set_visible(false)` 走 `WM_USER_HIDE_TRAYICON` 消息路径，
/// 触发 tray-icon 内部的 `remove_tray_icon` → `Shell_NotifyIconW(NIM_DELETE)`，
/// 在进程结束前干净地把图标摘掉。其它平台 `set_visible(false)` 也是
/// 正常的隐藏/移除语义，作为跨平台兜底也安全。
pub(crate) fn remove_tray_icon_before_exit(app_handle: &tauri::AppHandle) {
    if let Some(tray) = app_handle.tray_by_id(tray::TRAY_ID) {
        if let Err(e) = tray.set_visible(false) {
            log::warn!("退出时移除托盘图标失败: {e}");
        } else {
            log::info!("已显式从系统托盘移除图标");
        }
    }
}

/// setup 阶段用户选择退出：释放单实例锁后再 `process::exit`。
/// 直接 exit 不会跑插件的 `RunEvent::Exit` 钩子，macOS/Linux 的 sock 锁否则会残留，
/// 下次启动可能被当成第二个实例立刻退出。
fn exit_after_setup_failure(app: &tauri::AppHandle, code: i32) -> ! {
    destroy_single_instance_lock(app);
    std::process::exit(code);
}

/// 退出前等在途数据库任务放掉连接。
/// 固定 sleep 与落盘无关；这里通过一次短查询串行化到连接锁之后。
pub(crate) async fn wait_for_db_quiescent(app_handle: &tauri::AppHandle) {
    let Some(state) = app_handle.try_state::<store::AppState>() else {
        return;
    };
    let db = state.db.clone();
    match tauri::async_runtime::spawn_blocking(move || db.is_providers_empty()).await {
        Ok(_) => log::debug!("退出前已等待数据库连接空闲"),
        Err(e) => log::warn!("退出前等待数据库空闲失败: {e}"),
    }
}

/// 启动后期 Codex 历史归桶迁移（在阻塞线程上调用）。
fn run_codex_history_migrations_on_startup(db: &crate::database::Database) {
    match crate::codex_history_migration::maybe_migrate_codex_third_party_history_provider_bucket(
        db,
    ) {
        Ok(outcome) => {
            if let Some(reason) = outcome.skipped_reason {
                log::debug!("○ Codex history provider bucket migration skipped: {reason}");
            } else {
                log::info!(
                    "✓ Codex history provider bucket migration completed: sources={}, jsonl_files={}, state_rows={}",
                    outcome.source_provider_ids.len(),
                    outcome.migrated_jsonl_files,
                    outcome.migrated_state_rows
                );
            }
        }
        Err(e) => {
            log::warn!("✗ Codex history provider bucket migration failed: {e}");
        }
    }

    match crate::codex_history_migration::maybe_migrate_codex_provider_template_bucket(db) {
        Ok(outcome) => {
            if let Some(reason) = outcome.skipped_reason {
                log::debug!("○ Codex provider template bucket migration skipped: {reason}");
            } else if !outcome.migrated_provider_ids.is_empty() {
                log::info!(
                    "✓ Codex provider template bucket migration completed: providers={}",
                    outcome.migrated_provider_ids.len()
                );
            }
        }
        Err(e) => {
            log::warn!("✗ Codex provider template bucket migration failed: {e}");
        }
    }

    // 旧版官方路由桶（cc-switch-official）迁回 openai，须在统一迁移之前：
    // 开关开着时，紧接着的统一迁移会把它们按正常流程带进 custom。
    match crate::codex_history_migration::maybe_migrate_codex_legacy_official_history_to_openai_bucket(
    ) {
        Ok(outcome) => {
            if let Some(reason) = outcome.skipped_reason {
                log::debug!("○ Codex legacy official history bucket migration skipped: {reason}");
            } else {
                log::info!(
                    "✓ Codex legacy official history bucket migration completed: jsonl_files={}, state_rows={}",
                    outcome.migrated_jsonl_files,
                    outcome.migrated_state_rows
                );
            }
        }
        Err(e) => {
            log::warn!("✗ Codex legacy official history bucket migration failed: {e}");
        }
    }

    // 统一会话开关的官方历史迁移：开关开启但上次未完成（如文件被占用
    // 中途失败）时在启动期重试；函数内部自门控，开关关闭时直接跳过。
    match crate::codex_history_migration::maybe_migrate_codex_official_history_to_unified_bucket() {
        Ok(outcome) => {
            if let Some(reason) = outcome.skipped_reason {
                log::debug!("○ Codex official history unify migration skipped: {reason}");
            } else {
                log::info!(
                    "✓ Codex official history unify migration completed: jsonl_files={}, state_rows={}",
                    outcome.migrated_jsonl_files,
                    outcome.migrated_state_rows
                );
            }
        }
        Err(e) => {
            log::warn!("✗ Codex official history unify migration failed: {e}");
        }
    }
}

// ============================================================
// 迁移错误对话框辅助函数
// ============================================================

/// 检测是否为中文环境（与托盘语言探测对齐：优先 sys_locale，再回退 LANG）。
fn is_chinese_locale() -> bool {
    if let Some(locale) = sys_locale::get_locale() {
        let locale = locale.to_ascii_lowercase();
        if locale.is_empty() {
            // 与 tray::map_locale_to_tray_language("") -> "zh" 一致
            return true;
        }
        return locale.starts_with("zh");
    }

    std::env::var("LANG")
        .or_else(|_| std::env::var("LC_ALL"))
        .or_else(|_| std::env::var("LC_MESSAGES"))
        .map(|lang| lang.to_ascii_lowercase().starts_with("zh"))
        // 取不到区域时与托盘默认中文一致，避免 Windows 无 LANG 时误显英文致命对话框
        .unwrap_or(true)
}

/// 初始化 worker 可等待原生选择；用户关闭主窗口时也能退出，不依赖弹框回调。
fn wait_for_recovery_choice(dialog: tauri_plugin_dialog::MessageDialogBuilder<tauri::Wry>) -> bool {
    if USER_EXIT_IN_PROGRESS.load(Ordering::SeqCst) {
        return false;
    }
    let (send, receive) = std::sync::mpsc::channel();
    dialog.show(move |retry| {
        let _ = send.send(retry);
    });
    loop {
        if USER_EXIT_IN_PROGRESS.load(Ordering::SeqCst) {
            return false;
        }
        match receive.recv_timeout(std::time::Duration::from_millis(100)) {
            Ok(retry) => return retry,
            Err(std::sync::mpsc::RecvTimeoutError::Timeout) => {}
            Err(std::sync::mpsc::RecvTimeoutError::Disconnected) => return false,
        }
    }
}

/// 显示迁移错误对话框
/// 返回 true 表示用户选择重试，false 表示用户选择退出
fn show_migration_error_dialog(app: &tauri::AppHandle, error: &str) -> bool {
    let title = if is_chinese_locale() {
        "配置迁移失败"
    } else {
        "Migration Failed"
    };

    let message = if is_chinese_locale() {
        format!(
            "从旧版本迁移配置时发生错误：\n\n{error}\n\n\
            您的数据尚未丢失，旧配置文件仍然保留。\n\
            建议回退到旧版本 CC Switch 以保护数据。\n\n\
            点击「重试」重新尝试迁移\n\
            点击「退出」关闭程序（可回退版本后重新打开）"
        )
    } else {
        format!(
            "An error occurred while migrating configuration:\n\n{error}\n\n\
            Your data is NOT lost - the old config file is still preserved.\n\
            Consider rolling back to an older CC Switch version.\n\n\
            Click 'Retry' to attempt migration again\n\
            Click 'Exit' to close the program"
        )
    };

    let retry_text = if is_chinese_locale() {
        "重试"
    } else {
        "Retry"
    };
    let exit_text = if is_chinese_locale() {
        "退出"
    } else {
        "Exit"
    };

    // 仅初始化 worker 等待回调，主事件循环继续响应。
    // OkCancelCustom: 第一个按钮（重试）返回 true，第二个按钮（退出）返回 false
    wait_for_recovery_choice(
        app.dialog()
            .message(&message)
            .title(title)
            .kind(MessageDialogKind::Error)
            .buttons(MessageDialogButtons::OkCancelCustom(
                retry_text.to_string(),
                exit_text.to_string(),
            )),
    )
}

/// 显示数据库初始化/Schema 迁移失败对话框
/// 返回 true 表示用户选择重试，false 表示用户选择退出
fn show_database_init_error_dialog(
    app: &tauri::AppHandle,
    db_path: &std::path::Path,
    error: &str,
) -> bool {
    let title = if is_chinese_locale() {
        "数据库初始化失败"
    } else {
        "Database Initialization Failed"
    };

    let message = if is_chinese_locale() {
        format!(
            "初始化数据库或迁移数据库结构时发生错误：\n\n{error}\n\n\
            数据库文件路径：\n{db}\n\n\
            您的数据尚未丢失，应用不会自动删除数据库文件。\n\
            常见原因包括：数据库版本过新、文件损坏、权限不足、磁盘空间不足等。\n\n\
            建议：\n\
            1) 先备份整个配置目录（包含 cc-switch.db）\n\
            2) 如果提示“数据库版本过新”，请升级到更新版本\n\
            3) 如果刚升级出现异常，可回退旧版本导出/备份后再升级\n\n\
            点击「重试」重新尝试初始化\n\
            点击「退出」关闭程序",
            db = db_path.display()
        )
    } else {
        format!(
            "An error occurred while initializing or migrating the database:\n\n{error}\n\n\
            Database file path:\n{db}\n\n\
            Your data is NOT lost - the app will not delete the database automatically.\n\
            Common causes include: newer database version, corrupted file, permission issues, or low disk space.\n\n\
            Suggestions:\n\
            1) Back up the entire config directory (including cc-switch.db)\n\
            2) If you see “database version is newer”, please upgrade CC Switch\n\
            3) If this happened right after upgrading, consider rolling back to export/backup then upgrade again\n\n\
            Click 'Retry' to attempt initialization again\n\
            Click 'Exit' to close the program",
            db = db_path.display()
        )
    };

    let retry_text = if is_chinese_locale() {
        "重试"
    } else {
        "Retry"
    };
    let exit_text = if is_chinese_locale() {
        "退出"
    } else {
        "Exit"
    };

    wait_for_recovery_choice(
        app.dialog()
            .message(&message)
            .title(title)
            .kind(MessageDialogKind::Error)
            .buttons(MessageDialogButtons::OkCancelCustom(
                retry_text.to_string(),
                exit_text.to_string(),
            )),
    )
}

// ============================================================
// 退出请求分类
// ============================================================

/// `RunEvent::ExitRequested` 的三类来源，处理方式必须区分。
///
/// 关键约束：重启请求（`code == RESTART_EXIT_CODE`）上 `prevent_exit()` 会被
/// Tauri 静默忽略（见 `ExitRequestApi::prevent_exit` 文档），事件循环必定继续
/// 退出并触发各插件的 `RunEvent::Exit` 钩子；任何与之并发的自定义清理任务都
/// 可能与插件退出钩子争用同一状态而死锁。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ExitRequestAction {
    /// `code` 为 `None`：运行时自动触发（如隐藏窗口的 WebView 被回收导致无存活
    /// 窗口），阻止退出、保持托盘后台运行。
    StayInTray,
    /// `code` 为 `RESTART_EXIT_CODE`：`app.restart()` 发起的重启（本应用自己的
    /// 重启都走 `restart_process`，这里只兜底），不拦截、不做自定义清理，交还
    /// Tauri 默认 re-exec 流程。
    DeferToTauriRestart,
    /// 其它 `Some(_)`：用户主动退出（托盘「退出」等），执行完整异步清理后结束进程。
    CleanupAndExit,
}

/// 收到过重启请求。重启时 Tauri 也会发 `RunEvent::Exit`，靠它跳过系统终止那条清理。
static RESTART_REQUESTED: AtomicBool = AtomicBool::new(false);

fn classify_exit_request(code: Option<i32>) -> ExitRequestAction {
    match code {
        None => ExitRequestAction::StayInTray,
        Some(tauri::RESTART_EXIT_CODE) => ExitRequestAction::DeferToTauriRestart,
        Some(_) => ExitRequestAction::CleanupAndExit,
    }
}

// ============================================================
// 在应用主动退出前显式持久化窗口状态
// ============================================================

fn window_state_flags() -> StateFlags {
    StateFlags::POSITION | StateFlags::SIZE | StateFlags::MAXIMIZED
}

/// 当前应用的退出路径会拦截 `ExitRequested` 并最终直接 `std::process::exit(0)`，
/// 这里需要在真正结束进程前手动落盘，避免 window-state 插件的默认退出钩子被绕过。
pub fn save_window_state_before_exit(app_handle: &tauri::AppHandle) {
    if let Err(err) = app_handle.save_window_state(window_state_flags()) {
        log::error!("退出前保存窗口状态失败: {err}");
    } else {
        log::info!("已在退出前保存窗口状态");
    }
}

/// 主动释放 single-instance 锁。
///
/// macOS single-instance 使用 `/tmp/{identifier}.sock`。我们有若干路径会直接
/// `std::process::exit(0)`，不会触发插件挂在 `RunEvent::Exit` 上的清理钩子。
/// 重启前主动 destroy 可以避免新进程误连旧 listener 后自行退出。
pub fn destroy_single_instance_lock(app_handle: &tauri::AppHandle) {
    #[cfg(any(target_os = "macos", target_os = "windows", target_os = "linux"))]
    tauri_plugin_single_instance::destroy(app_handle);
}

/// 清理托盘图标、释放 single-instance 锁后重启当前应用。
///
/// 直接 spawn 新进程 + `exit(0)`（macOS 经 `open -n`，见 `relaunch_macos_bundle`），不经过事件
/// 循环退出，因此 Tauri 内部的 `cleanup_before_exit` 和各插件的
/// `RunEvent::Exit` 钩子都不会执行。需要的清理由调用方与本函数显式补偿：
/// 窗口状态、代理/Live 恢复（调用方）；托盘图标、single-instance 锁（本函数）。
///
/// 有意不调 `AppHandle::cleanup_before_exit()`：它会在调用线程上 Drop 托盘
/// 图标，而 macOS 的 NSStatusItem 操作要求主线程；`set_visible(false)` 走
/// `run_item_main_thread` 代理，跨线程安全（见 `remove_tray_icon_before_exit`）。
pub fn restart_process(app_handle: &tauri::AppHandle) -> ! {
    remove_tray_icon_before_exit(app_handle);
    destroy_single_instance_lock(app_handle);
    // 丢掉本次启动带来的 ccswitch://，避免改目录 / 装更新后的重启再次弹出导入。
    let env = env_without_deeplink_args(app_handle.env());
    #[cfg(target_os = "macos")]
    relaunch_macos_bundle(&env);
    tauri::process::restart(&env);
}

/// 复制 Env 并去掉深链接参数，供重启使用。
fn env_without_deeplink_args(mut env: tauri::Env) -> tauri::Env {
    env.args_os
        .retain(|arg| !arg.to_string_lossy().starts_with("ccswitch://"));
    env
}

/// macOS 经 LaunchServices（`open -n`）启动新实例，成功即退出；失败时返回，
/// 由调用方回落到 `tauri::process::restart`。
///
/// `tauri::process::restart` 直接 spawn 可执行文件。macOS 14 起应用激活是协作式的：
/// 新进程的 `activateIgnoringOtherApps` 会被系统拒绝，窗口留在其它应用后面。
/// 由当前前台应用请求 LaunchServices 启动，新实例才能拿到前台。
#[cfg(target_os = "macos")]
fn relaunch_macos_bundle(env: &tauri::Env) {
    let Ok(binary) = tauri::process::current_binary(env) else {
        return;
    };
    // <Name>.app/Contents/MacOS/<binary>
    let Some(bundle) = binary
        .ancestors()
        .nth(3)
        .filter(|p| p.extension().is_some_and(|ext| ext == "app"))
    else {
        return;
    };
    let mut command = std::process::Command::new("/usr/bin/open");
    command.arg("-n").arg(bundle);
    let args: Vec<_> = env.args_os.iter().skip(1).collect();
    if !args.is_empty() {
        command.arg("--args").args(args);
    }
    match command.status() {
        Ok(status) if status.success() => std::process::exit(0),
        Ok(status) => log::warn!("open -n 重启失败（{status}），回落直接启动"),
        Err(err) => log::warn!("open -n 重启失败（{err}），回落直接启动"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        classify_exit_request, env_without_deeplink_args, error_for_log, redact_url_for_log,
        redact_url_for_log_with_secrets, redact_url_origin_for_log, runtime_log_level_allows,
        ExitRequestAction,
    };

    #[test]
    fn restart_env_drops_deeplink_args_keeps_others() {
        let mut env = tauri::Env::default();
        env.args_os = vec![
            "cc-switch".into(),
            "--flag".into(),
            "ccswitch://import/provider?x=1".into(),
            "plain-arg".into(),
        ];
        let filtered = env_without_deeplink_args(env);
        let args: Vec<_> = filtered
            .args_os
            .iter()
            .map(|a| a.to_string_lossy().into_owned())
            .collect();
        assert_eq!(args, vec!["cc-switch", "--flag", "plain-arg"]);
    }

    #[cfg(unix)]
    #[test]
    fn raise_open_file_limit_lifts_soft_limit_to_target_or_hard_limit() {
        fn current() -> libc::rlimit {
            let mut limit = libc::rlimit {
                rlim_cur: 0,
                rlim_max: 0,
            };
            assert_eq!(
                unsafe { libc::getrlimit(libc::RLIMIT_NOFILE, &mut limit) },
                0
            );
            limit
        }

        let before = current();
        super::raise_open_file_limit();
        let after = current();
        let expected = super::OPEN_FILE_SOFT_LIMIT_TARGET.min(after.rlim_max);
        assert!(after.rlim_cur >= expected);
        assert!(after.rlim_cur >= before.rlim_cur);
        assert_eq!(after.rlim_max, before.rlim_max);
    }

    #[test]
    fn log_error_drops_toml_source_lines_but_keeps_position() {
        let secret = "sk-review-only-secret";
        let toml_edit_error = format!("experimental_bearer_token = \"{secret}\" !\n")
            .parse::<toml_edit::DocumentMut>()
            .unwrap_err()
            .to_string();
        let toml_error = toml::from_str::<toml::Table>(&format!("token = \"{secret}\" !\n"))
            .unwrap_err()
            .to_string();
        for error in [toml_edit_error, toml_error] {
            assert!(error.contains(secret), "前提：诊断里带源码行");
            let logged = error_for_log(&format!("无法解析：{error} (cannot parse: {error})"));
            assert!(!logged.contains(secret), "{logged}");
            assert!(logged.contains("line 1"), "{logged}");
        }
        // 普通错误原样保留。
        assert_eq!(error_for_log("供应商 a 不存在"), "供应商 a 不存在");
    }

    #[test]
    fn log_url_redaction_strips_credentials_and_query_keeps_path() {
        // userinfo 与整个 query 剥离，path 保留用于诊断 base_url 配错。
        assert_eq!(
            redact_url_for_log(
                "https://user:secret@example.com:8443/v1/models?key=top-secret&alt=sse"
            ),
            "https://example.com:8443/v1/models"
        );
        // scheme-relative 保持形态，userinfo 去掉。
        assert_eq!(
            redact_url_for_log("//user:sk-secret@gw.example.com/v1"),
            "//gw.example.com/v1"
        );
        // 无 scheme 的裸 userinfo。
        assert_eq!(
            redact_url_for_log("user:sk-secret@gw.example.com/v1"),
            "gw.example.com/v1"
        );
        // 无法解析为绝对 URL 时：丢 query，其余原样保留。
        assert_eq!(redact_url_for_log("not-a-url?token=secret"), "not-a-url");
        // 不再对 path 段做“看起来像密钥”的形状猜测，正常路径完整保留。
        assert_eq!(
            redact_url_for_log("https://host.example/v1/models/gemini-2.5-pro"),
            "https://host.example/v1/models/gemini-2.5-pro"
        );
    }

    #[test]
    fn log_url_redaction_replaces_known_secret_values() {
        // 精确匹配已知密钥值：无论它出现在 path 还是别处都被抹掉。
        let secrets = vec!["k-9f3a7c2b1e".to_string()];
        assert_eq!(
            redact_url_for_log_with_secrets("https://gw.example.com/k-9f3a7c2b1e/v1", &secrets),
            "https://gw.example.com/[REDACTED]/v1"
        );
        // 过短(<8)的已知值不参与子串脱敏，避免误伤 /v1/ 之类的正常路径。
        let short_secrets = vec!["api".to_string()];
        assert_eq!(
            redact_url_for_log_with_secrets("https://api.example.com/v1", &short_secrets),
            "https://api.example.com/v1"
        );
    }

    #[test]
    fn log_url_origin_drops_path_for_credential_in_path() {
        // 没有已知密钥可脱敏时，凭据可能整个内嵌在 path，只记 origin。
        assert_eq!(
            redact_url_origin_for_log("https://gw.example.com/k-9f3a7c2b1e/v1"),
            "https://gw.example.com"
        );
        assert_eq!(
            redact_url_origin_for_log("https://user:pass@gw.example.com:8443/secret/v1"),
            "https://gw.example.com:8443"
        );
        assert_eq!(
            redact_url_origin_for_log("//gw.example.com/secret/v1"),
            "//gw.example.com"
        );
    }

    #[test]
    fn runtime_log_filter_honors_dynamic_max_level() {
        assert!(!runtime_log_level_allows(
            log::Level::Error,
            log::LevelFilter::Off
        ));
        assert!(runtime_log_level_allows(
            log::Level::Error,
            log::LevelFilter::Info
        ));
        assert!(runtime_log_level_allows(
            log::Level::Info,
            log::LevelFilter::Info
        ));
        assert!(!runtime_log_level_allows(
            log::Level::Debug,
            log::LevelFilter::Info
        ));
    }

    #[test]
    fn no_code_keeps_app_alive_in_tray() {
        assert_eq!(classify_exit_request(None), ExitRequestAction::StayInTray);
    }

    #[test]
    fn restart_exit_code_defers_to_tauri_default_restart() {
        assert_eq!(
            classify_exit_request(Some(tauri::RESTART_EXIT_CODE)),
            ExitRequestAction::DeferToTauriRestart
        );
    }

    #[test]
    fn user_exit_codes_run_cleanup_then_exit() {
        assert_eq!(
            classify_exit_request(Some(0)),
            ExitRequestAction::CleanupAndExit
        );
        assert_eq!(
            classify_exit_request(Some(1)),
            ExitRequestAction::CleanupAndExit
        );
    }
}
