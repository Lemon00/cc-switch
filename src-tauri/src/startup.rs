//! GUI 启动编排：在 blocking 线程保序执行磁盘初始化，主事件循环保持可用。
//! 前端 get_init_error 等待本阶段结束后才挂载业务页面。
use super::*;

static INTERACTIVE_LAUNCH: AtomicBool = AtomicBool::new(false);

pub(crate) fn request_main_window() {
    INTERACTIVE_LAUNCH.store(true, Ordering::SeqCst);
}

fn ensure_main_window_for_error(app: &tauri::AppHandle) {
    match crate::lightweight::create_main_window(app) {
        Ok(window) => {
            #[cfg(target_os = "windows")]
            let _ = window.set_skip_taskbar(false);
            let _ = window.show();
            let _ = window.set_focus();
        }
        Err(error) => {
            log::error!("创建恢复界面主窗口失败: {error}");
        }
    }
}

pub(crate) fn start(app: tauri::AppHandle) {
    tauri::async_runtime::spawn(async move {
        let handle = app.clone();
        let result = tauri::async_runtime::spawn_blocking(move || {
            initialize(&handle).map_err(|error| error.to_string())
        })
        .await;
        let error = match result {
            Ok(Ok(())) => None,
            Ok(Err(error)) => Some(error),
            Err(error) => Some(format!("Startup task failed: {error}")),
        };
        if let Some(error) = error {
            log::error!("Initialization failed: {error}");
            crate::init_status::set_init_error(crate::init_status::InitErrorPayload {
                path: crate::config::get_app_config_dir().display().to_string(),
                error,
                kind: Some("startup_failed".into()),
                db_version: None,
                supported_version: None,
            });
            ensure_main_window_for_error(&app);
        }
        crate::init_status::mark_startup_ready();
        crate::trim_process_memory();
        if crate::init_status::get_init_error().is_none()
            && !USER_EXIT_IN_PROGRESS.load(Ordering::SeqCst)
        {
            start_background_jobs(&app);
        }
    });
}

fn initialize(app: &tauri::AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    // 初始化数据库
    let app_config_dir = crate::config::get_app_config_dir();
    let db_path = app_config_dir.join("cc-switch.db");
    let json_path = app_config_dir.join("config.json");

    // 检查是否需要从 config.json 迁移到 SQLite
    let has_json = json_path.exists();
    let has_db = db_path.exists();

    // 如果需要迁移，先验证 config.json 是否可以加载（在创建数据库之前）
    // 这样如果加载失败用户选择退出，数据库文件还没被创建，下次可以正常重试
    let mut migration_config = if !has_db && has_json {
        log::info!("检测到旧版配置文件，验证配置文件...");

        // 循环：支持用户重试加载配置文件
        loop {
            match crate::app_config::MultiAppConfig::load() {
                Ok(config) => {
                    log::info!("✓ 配置文件加载成功");
                    break Some(config);
                }
                Err(e) => {
                    log::error!("加载旧配置文件失败: {e}");
                    // 弹出系统对话框让用户选择
                    if !show_migration_error_dialog(app, &e.to_string()) {
                        // 用户选择退出（此时数据库还没创建，下次启动可以重试）
                        log::info!("用户选择退出程序");
                        exit_after_setup_failure(app, 1);
                    }
                    // 用户选择重试，继续循环
                    log::info!("用户选择重试加载配置文件");
                }
            }
        }
    } else {
        None
    };

    // 现在创建数据库（包含 Schema 迁移）
    //
    // 说明：从 v3.8.* 升级的用户通常会走到这里的 SQLite schema 迁移，
    // 若迁移失败（数据库损坏/权限不足/user_version 过新等），需要给用户明确提示，
    // 否则表现可能只是“应用打不开/闪退”。
    //
    // 预检：数据库版本过新时，必须先于任何 schema 写操作（create_tables 内含
    // DROP/ALTER 等 DDL）进入恢复界面，避免旧应用对读不懂的更新版 DB 落写。
    match crate::database::Database::stored_user_version_exceeds_supported(&db_path) {
        Ok(Some(version)) => {
            log::warn!("数据库版本过新（v{version}），引导用户在应用内升级应用");
            crate::init_status::set_init_error(crate::init_status::InitErrorPayload {
                path: db_path.display().to_string(),
                error: format!(
                    "数据库版本过新（{version}），当前应用仅支持 {}，请升级应用后再尝试。",
                    crate::database::SCHEMA_VERSION
                ),
                kind: Some("db_version_too_new".to_string()),
                db_version: Some(version),
                supported_version: Some(crate::database::SCHEMA_VERSION),
            });
            // 主窗口默认按需或延迟显示，恢复界面必须强制拉起主窗口
            ensure_main_window_for_error(app);
            return Ok(());
        }
        Ok(None) => {}
        Err(e) => {
            log::warn!("预检数据库版本失败，fail-closed 暂停 schema 初始化: {e}");
            crate::init_status::set_init_error(crate::init_status::InitErrorPayload {
                path: db_path.display().to_string(),
                error: format!(
                    "无法确认数据库版本是否兼容当前应用（{e}）。请检查文件权限与磁盘状态后重试，或升级应用。"
                ),
                kind: Some("db_version_check_failed".to_string()),
                db_version: None,
                supported_version: Some(crate::database::SCHEMA_VERSION),
            });
            ensure_main_window_for_error(app);
            return Ok(());
        }
    }

    let db = loop {
        match cc_switch_core::startup::open_database() {
            Ok(db) => break Arc::new(db),
            Err(e) => {
                log::error!("Failed to init database: {e}");

                if !show_database_init_error_dialog(app, &db_path, &e.to_string()) {
                    log::info!("用户选择退出程序");
                    exit_after_setup_failure(app, 1);
                }

                log::info!("用户选择重试初始化数据库");
            }
        }
    };

    // 数据库可用后立即应用持久化日志级别，避免后续服务初始化
    // 继续使用启动阶段的 Info 回退。损坏配置显式 fail-closed 到 Info。
    match db.get_log_config() {
        Ok(log_config) => {
            log::set_max_level(log_config.to_level_filter());
            log::info!(
                "已加载日志配置: enabled={}, level={}",
                log_config.enabled,
                log_config.level
            );
        }
        Err(e) => {
            log::set_max_level(log::LevelFilter::Info);
            log::warn!("读取日志配置失败，已回退到 info: {e}");
        }
    }

    // 跨次启动补救：上次 migrate_from_json 失败时 DB 已存在但 config.json
    // 未归档。若供应商表仍空，说明迁移没完成，允许再读 json 重试；
    // 若已有供应商，不再自动灌入，避免覆盖用户已在新库里的数据。
    if migration_config.is_none() && has_json {
        let providers_empty = db.is_providers_empty().unwrap_or(false);
        if providers_empty {
            log::warn!("检测到未归档的 config.json 且数据库供应商为空，重试 JSON→SQLite 迁移");
            migration_config = loop {
                match crate::app_config::MultiAppConfig::load() {
                    Ok(config) => {
                        log::info!("✓ 配置文件加载成功（跨次重试）");
                        break Some(config);
                    }
                    Err(e) => {
                        log::error!("重试加载旧配置文件失败: {e}");
                        if !show_migration_error_dialog(app, &e.to_string()) {
                            log::info!("用户选择退出程序");
                            exit_after_setup_failure(app, 1);
                        }
                        log::info!("用户选择重试加载配置文件");
                    }
                }
            };
        } else {
            log::warn!(
                "配置目录仍有 config.json，但数据库已有供应商数据；跳过自动迁移以免覆盖。如不再需要可手动改名为 config.json.migrated"
            );
        }
    }

    // 执行 JSON→SQLite 迁移：失败时弹窗允许重试，退出则保留 config.json
    // 供下次启动继续（见上方跨次补救）。成功才归档。
    if let Some(config) = migration_config {
        log::info!("开始执行数据迁移...");

        loop {
            match db.migrate_from_json(&config) {
                Ok(_) => {
                    log::info!("✓ 配置迁移成功");
                    // 标记迁移成功，供前端显示 Toast
                    crate::init_status::set_migration_success();
                    // 归档旧配置文件（重命名而非删除，便于用户恢复）
                    let archive_path = json_path.with_extension("json.migrated");
                    if let Err(e) = std::fs::rename(&json_path, &archive_path) {
                        log::warn!("归档旧配置文件失败: {e}");
                    } else {
                        log::info!("✓ 旧配置已归档为 config.json.migrated");
                    }
                    break;
                }
                Err(e) => {
                    log::error!("配置迁移失败: {e}");
                    if !show_migration_error_dialog(app, &e.to_string()) {
                        log::info!("用户选择退出程序；保留 config.json 以便下次重试");
                        exit_after_setup_failure(app, 1);
                    }
                    log::info!("用户选择重试配置迁移");
                }
            }
        }
    }

    let app_state = AppState::new(db);

    // 给核心注入 GUI 钩子：事件转发、OAuth 读取、托盘刷新、
    // 故障转移统一由核心执行器处理，使用弱引用共享状态。
    app_state
        .proxy_service
        .set_hooks(frontend_hooks::tauri_hooks(app, &app_state));
    app.manage(app_state.clone());

    // 补完上次崩溃时写到一半的客户端文件（写前意图在 ~/.cc-switch/live-state.json），
    // 要在任何写客户端文件的启动步骤之前。
    crate::mode::operation::recover_on_startup(&app_state.db);
    // 上次退出或崩溃前没来得及删的 Claude 空值墓碑（切换时给开着的会话写的）接着删
    crate::services::provider::claude_direct::resume_tombstone_sweep();

    // ============================================================
    // 按表独立判断的导入逻辑（各类数据独立检查，互不影响）
    // ============================================================

    // 1. 初始化默认 Skills 仓库（已有内置检查：表非空则跳过）
    match app_state.db.init_default_skill_repos() {
        Ok(count) if count > 0 => {
            log::info!("✓ Initialized {count} default skill repositories");
        }
        Ok(_) => {} // 表非空，静默跳过
        Err(e) => log::warn!("✗ Failed to initialize default skill repos: {e}"),
    }

    // 1.1. Skills 统一管理迁移：当数据库迁移到 v3 结构后，自动从各应用目录导入到 SSOT
    // 触发条件由 schema 迁移设置 settings.skills_ssot_migration_pending = true 控制。
    match app_state.db.get_setting("skills_ssot_migration_pending") {
        Ok(Some(flag)) if flag == "true" || flag == "1" => {
            // 迁移合并既有记录并原子提交，兼容旧版本留下的部分迁移结果。
            match crate::services::skill::migrate_skills_to_ssot(&app_state.db) {
                Ok(count) => {
                    log::info!("✓ Auto imported {count} skill(s) into SSOT");
                    if count > 0 {
                        crate::init_status::set_skills_migration_result(count);
                    }
                }
                Err(e) => {
                    log::warn!("✗ Failed to auto import legacy skills to SSOT: {e}");
                    crate::init_status::set_skills_migration_error(e.to_string());
                }
            }
        }
        Ok(_) => {} // 未开启迁移标志，静默跳过
        Err(e) => log::warn!("✗ Failed to read skills migration flag: {e}"),
    }

    // 1.5. 自动导入 live 配置 + seed 官方预设供应商（Claude / Codex / Gemini）
    //
    // 先 import 后 seed 是有意为之：先把用户手动配置的 settings.json / auth.json / .env
    // 落成 "default" provider 设为 current，再追加官方预设（is_current=false）。
    // 降级回旧版时，旧版的回填和整份写入仍靠这一行保住用户原来的 live 配置。
    //
    // 捕获首次运行快照：所有全新装用户都会看到欢迎弹窗介绍 CC Switch 的工作方式。
    // 读失败时默认不弹，宁可漏弹也不要因为故障打扰用户。
    let first_run_already_confirmed = crate::settings::get_settings()
        .first_run_notice_confirmed
        .unwrap_or(false);
    let fresh_install_at_startup = app_state.db.is_providers_empty().unwrap_or(false);

    for app_type in crate::app_config::AppType::all().filter(|t| !t.is_additive_mode()) {
        if !crate::services::provider::should_import_default_config_on_startup(
            &app_state, &app_type,
        )
        .unwrap_or(false)
        {
            log::debug!(
                "○ {} already has providers; live import skipped",
                app_type.as_str()
            );
            continue;
        }

        match crate::services::provider::import_default_config(&app_state, app_type.clone()) {
            Ok(true) => log::info!(
                "✓ Imported live config for {} as default provider",
                app_type.as_str()
            ),
            Ok(false) => log::debug!(
                "○ {} already has providers; live import skipped",
                app_type.as_str()
            ),
            Err(e) => log::debug!("○ No live config to import for {}: {e}", app_type.as_str()),
        }
    }

    if let Err(error) = crate::services::provider::initialize_dsh_profiles(&app_state) {
        log::warn!("Initializing DeepSeek Harness profiles failed: {error}");
    }

    match app_state.db.init_default_official_providers() {
        Ok(count) if count > 0 => {
            log::info!("✓ Seeded {count} official provider(s)");
        }
        Ok(_) => {}
        Err(e) => log::warn!("✗ Failed to seed official providers: {e}"),
    }

    // Codex 历史归桶迁移改到 setup 完成、主窗口决策之后的异步恢复任务里跑：
    // 原先在这里 spawn_blocking 会与紧接着的供应商/MCP/提示词同步导入争用同一把
    // 数据库连接锁，大会话目录下表现为启动后长时间无窗口、无托盘。

    // 老用户 / 已确认的路径由 `fresh_install_at_startup` 自行拦截，这里不做写入。
    // 字段只由前端在用户点击"我知道了"时 save_settings 回写，语义是"用户显式确认过"。
    if !first_run_already_confirmed && fresh_install_at_startup {
        log::info!("✓ First-run welcome notice pending");
    }

    // 1.6. 自动同步累加模式应用的原生 providers 到数据库
    //
    // additive 模式的 import 函数按 id 幂等——
    // 新 id 执行导入，已有 id 则更新 settings 和 display name，所以每次
    // 启动都跑是安全的：既保证新装用户开箱可见 live 中的供应商，也让外部
    // 修改的 live 文件能在重启后同步到数据库（与之前依赖前端"导入当前配置"
    // 按钮手动触发不同）。
    //
    // 底层 read_*_config 在文件不存在时返回默认空配置，因此新装且无
    // live 文件的用户走 Ok(0) 路径，不会产生错误日志噪音。
    match crate::services::provider::import_opencode_providers_from_live(&app_state) {
        Ok(count) if count > 0 => {
            log::info!("✓ Synced {count} OpenCode provider(s) from live config");
        }
        Ok(_) => log::debug!("○ No OpenCode provider changes from live config"),
        Err(e) => log::warn!("✗ Failed to import OpenCode providers: {e}"),
    }

    // 2. OMO 配置导入（当数据库中无 OMO provider 时，从本地文件导入）
    {
        let has_omo = app_state
            .db
            .get_all_providers(&AppType::OpenCode)
            .map(|providers| {
                providers
                    .values()
                    .any(|p| p.category.as_deref() == Some("omo"))
            })
            // 读不出 OpenCode 的供应商时不导入：没法确认已有，导入会每次启动都多加一份。
            .unwrap_or(true);
        if !has_omo {
            match crate::services::OmoService::import_from_local(
                &app_state,
                &crate::services::omo::STANDARD,
            ) {
                Ok(provider) => {
                    log::info!(
                        "✓ Imported OMO config from local as provider '{}'",
                        provider.name
                    );
                }
                Err(AppError::OmoConfigNotFound) => {
                    log::debug!("○ No OMO config to import");
                }
                Err(e) => {
                    log::warn!("✗ Failed to import OMO config from local: {e}");
                }
            }
        }
    }

    // 2.3 OMO Slim config import (when no omo-slim provider in DB, import from local)
    {
        let has_omo_slim = app_state
            .db
            .get_all_providers(&AppType::OpenCode)
            .map(|providers| {
                providers
                    .values()
                    .any(|p| p.category.as_deref() == Some("omo-slim"))
            })
            // 读不出 OpenCode 的供应商时不导入：没法确认已有，导入会每次启动都多加一份。
            .unwrap_or(true);
        if !has_omo_slim {
            match crate::services::OmoService::import_from_local(
                &app_state,
                &crate::services::omo::SLIM,
            ) {
                Ok(provider) => {
                    log::info!(
                        "✓ Imported OMO Slim config from local as provider '{}'",
                        provider.name
                    );
                }
                Err(AppError::OmoConfigNotFound) => {
                    log::debug!("○ No OMO Slim config to import");
                }
                Err(e) => {
                    log::warn!("✗ Failed to import OMO Slim config from local: {e}");
                }
            }
        }
    }

    // 启动阶段不再无条件保存,避免意外覆盖用户配置。

    // 注册 deep-link URL 处理器（使用正确的 DeepLinkExt API）
    log::info!("=== Registering deep-link URL handler ===");

    // Linux 和 Windows 调试模式需要显式注册
    #[cfg(any(target_os = "linux", all(debug_assertions, windows)))]
    {
        #[cfg(target_os = "linux")]
        {
            // Use Tauri's path API to get correct path (includes app identifier)
            // tauri-plugin-deep-link writes to: ~/.local/share/com.ccswitch.desktop/applications/cc-switch-handler.desktop
            // Only register if .desktop file doesn't exist to avoid overwriting user customizations
            let should_register = app
                .path()
                .data_dir()
                .map(|d| !d.join("applications/cc-switch-handler.desktop").exists())
                .unwrap_or(true);

            if should_register {
                if let Err(e) = app.deep_link().register_all() {
                    log::error!("✗ Failed to register deep link schemes: {}", e);
                } else {
                    log::info!("✓ Deep link schemes registered (Linux)");
                }
            } else {
                log::info!("⊘ Deep link handler already exists, skipping registration");
            }
        }

        #[cfg(all(debug_assertions, windows))]
        {
            if let Err(e) = app.deep_link().register_all() {
                log::error!("✗ Failed to register deep link schemes: {}", e);
            } else {
                log::info!("✓ Deep link schemes registered (Windows debug)");
            }
        }
    }

    // 注册 URL 处理回调（所有平台通用）
    app.deep_link().on_open_url({
        let app_handle = app.clone();
        move |event| {
            log::info!("=== Deep Link Event Received (on_open_url) ===");
            let urls = event.urls();
            log::info!("Received {} URL(s)", urls.len());

            for (i, url) in urls.iter().enumerate() {
                let url_str = url.as_str();
                log::debug!("  URL[{i}]: {}", url_for_log(url_str));

                if handle_deeplink_url(&app_handle, url_str, true, "on_open_url") {
                    break; // Process only first ccswitch:// URL
                }
            }
        }
    });
    log::info!("✓ Deep-link URL handler registered");

    // 冷启动补取：Windows/Linux 上插件在自身初始化时就解析了启动参数并发出事件，
    // macOS 的 Opened 也可能早于这里，都赶在上面的回调注册之前，只能从 get_current 取。
    // 万一两边都处理了同一条链接也无妨：暂存区只留最新一条，前端处理完就清除。
    match app.deep_link().get_current() {
        Ok(Some(urls)) => {
            for url in urls {
                if handle_deeplink_url(app, url.as_str(), true, "get_current") {
                    break;
                }
            }
        }
        Ok(None) => {}
        Err(e) => log::warn!("读取启动时的深链接失败: {e}"),
    }

    // 仍在初始化 worker 内，串行完成导入后才恢复路由；不再留下未等待的子任务。
    let state = &app_state;
    if let Err(error) = cc_switch_core::startup::run_startup_imports(
        &state.db,
        "startup_mcp_import_pending_v1",
        cc_switch_core::startup::STARTUP_MCP_APPS,
        || state.db.is_mcp_table_empty(),
        |app| crate::services::mcp::McpService::import_from_app(state, app),
    ) {
        log::warn!("MCP startup import: {error}");
    }
    if let Err(error) = cc_switch_core::startup::run_startup_imports(
        &state.db,
        "startup_prompt_import_pending_v1",
        cc_switch_core::startup::STARTUP_PROMPT_APPS,
        || state.db.is_prompts_table_empty(),
        |app| {
            crate::services::prompt::PromptService::import_from_file_on_first_launch(
                state,
                app.clone(),
            )
        },
    ) {
        log::warn!("Prompt startup import: {error}");
    }

    // AppState 已在后台任务启动前注册；这里注册其余服务。

    // 初始化 SkillService
    let skill_service = SkillService::new();
    app.manage(commands::skill::SkillServiceState(Arc::new(skill_service)));

    // 初始化 CopilotAuthManager
    {
        use crate::proxy::providers::copilot_auth::CopilotAuthManager;
        use commands::CopilotAuthState;
        use tokio::sync::RwLock;

        let app_config_dir = crate::config::get_app_config_dir();
        let copilot_auth_manager = CopilotAuthManager::new(app_config_dir);
        app.manage(CopilotAuthState(Arc::new(RwLock::new(
            copilot_auth_manager,
        ))));
        log::info!("✓ CopilotAuthManager initialized");
    }

    // 初始化 CodexOAuthManager (ChatGPT Plus/Pro 反代)
    {
        use commands::CodexOAuthState;

        let codex_oauth_manager = app.state::<AppState>().codex_oauth_manager.clone();
        app.manage(CodexOAuthState(codex_oauth_manager));
        log::info!("✓ CodexOAuthManager initialized");
    }

    // 初始化 xAI OAuthManager (Grok API 反代)
    {
        use crate::proxy::providers::xai_oauth_auth::XaiOAuthManager;
        use commands::XaiOAuthState;
        use tokio::sync::RwLock;

        let app_config_dir = crate::config::get_app_config_dir();
        let xai_oauth_manager = XaiOAuthManager::new(app_config_dir);
        app.manage(XaiOAuthState(Arc::new(RwLock::new(xai_oauth_manager))));
        log::info!("✓ XaiOAuthManager initialized");
    }

    // Antigravity 官方账号（授权中心加号、切 agy 的全局登录）
    {
        use crate::services::agy_account::AgyAccountManager;
        let manager = AgyAccountManager::new(crate::config::get_app_config_dir());
        app.manage(commands::AgyAccountState(Arc::new(manager)));
    }

    // Claude Code 官方订阅账号（授权中心加号；每个账号一个 CLAUDE_CONFIG_DIR）
    {
        use crate::services::claude_account::ClaudeAccountManager;
        let data_dir = crate::config::get_app_config_dir();
        let manager = ClaudeAccountManager::new(data_dir.clone());
        app.manage(commands::ClaudeAccountState(Arc::new(manager)));
        let db = app.state::<AppState>().db.clone();
        {
            // 上次切换登录时崩溃、断电：先补完（配置操作在前面已补完，指针已落定）
            crate::services::provider::claude_official::recover_login_switch(&db);
            // 默认目录的设置、MCP 可能在 CC Switch 没运行时改过
            crate::services::claude_account::sync_all_shared(&data_dir);
        }
    }

    // GUI 和 CLI 复用同一出站代理初始化，保持网络行为一致。
    if let Err(error) = cc_switch_core::startup::init_outbound_http(&app.state::<AppState>().db) {
        log::error!("[GlobalProxy] Failed to initialize outbound HTTP: {error}");
    }

    // 异常退出恢复 + 代理状态自动恢复
    let app_handle = app.clone();
    let routing_task = tauri::async_runtime::spawn(async move {
        let state = app_handle.state::<AppState>();

        cc_switch_core::initialize_common_config_snippets(&state);

        // 定下各应用的直连 / 代理模式（处理旧版遗留的接管状态），再把代理模式的
        // 应用接上。要排在通用配置片段的自动提取之后：它读的是直连的 live。
        if !USER_EXIT_IN_PROGRESS.load(Ordering::SeqCst) {
            crate::mode::controller::startup(&state).await;
        }
        // #8268 测试版给 agy 装过包装脚本、改过 shell 启动文件，撤掉带标记的部分。
        if let Err(e) = crate::services::provider::agy_direct::migrate_legacy_launcher(&state.db) {
            log::warn!("清理旧版 agy 启动器失败: {e}");
        }
        // 启动流程走完：托盘这时才开始报「路由服务没在运行」，并记下退回直连的应用。
        crate::tray::mark_startup_settled(&app_handle);
    });

    // 最终退出与业务界面等待所有初始化写入结束。
    tauri::async_runtime::block_on(routing_task)?;
    if USER_EXIT_IN_PROGRESS.load(Ordering::SeqCst) {
        return Ok(());
    }

    // 构建托盘：Windows 左键打开主界面，右键默认弹出面板；macOS 左右键默认都弹出面板。
    // 设置里打开「原生托盘菜单」就挂原生菜单（`tray::apply_tray_style` 运行时切换）。
    // Linux（AppIndicator）不派发点击事件，只能出原生菜单，菜单里有一项打开面板。
    let native_menu = tray_panel::uses_native_menu();
    let mut tray_builder = TrayIconBuilder::with_id(tray::TRAY_ID)
        .on_tray_icon_event(tray::handle_tray_icon_event)
        .on_menu_event(|app, event| {
            tray::handle_tray_menu_event(app, &event.id.0);
        })
        .show_menu_on_left_click(tray::native_menu_on_left_click(native_menu));
    // 鼠标悬停提示（macOS 不设，见 `tray::TRAY_TOOLTIP_ENABLED`）
    if tray::TRAY_TOOLTIP_ENABLED {
        tray_builder = tray_builder.tooltip("CC Switch");
    }
    if native_menu {
        let menu = tray::create_tray_menu(app, &app_state)?;
        tray_builder = tray_builder.menu(&menu);
    }

    // 使用平台对应的托盘图标（macOS 使用模板图标适配深浅色）；出问题时 tray.rs 换成带圆点的那张
    if let Some((icon, template)) = tray::base_tray_icon(app) {
        tray_builder = tray_builder.icon(icon).icon_as_template(template);
    }

    // Linux 托盘需要把图标写入磁盘。WSL 的 XDG_RUNTIME_DIR 可能指向
    // 不存在或不可写的目录，使用应用缓存目录避免因此中止启动。
    #[cfg(target_os = "linux")]
    {
        tray_builder = tray_builder.temp_dir_path(app.path().app_cache_dir()?.join("tray-icon"));
    }

    let _tray = tray_builder.build(app)?;
    #[cfg(target_os = "windows")]
    {
        let handle = app.clone();
        app.run_on_main_thread(move || tray_compat::install(&handle))?;
    }

    // 静默启动 / 轻量模式启动：先判定可见性，轻量模式下不创建主窗口以免启动额外 WebView 进程。
    let settings = crate::settings::get_settings();
    let interactive = INTERACTIVE_LAUNCH.load(Ordering::SeqCst);
    let visibility = crate::lifecycle::startup_visibility(
        settings.lightweight_mode,
        settings.silent_startup,
        interactive,
    );
    if visibility == crate::lifecycle::StartupVisibility::Lightweight {
        if let Err(e) = crate::lightweight::enter_lightweight_mode(app) {
            log::error!("启动时恢复轻量模式失败: {e}");
        } else {
            log::info!("上次退出时在轻量模式，启动后已恢复");
        }
    } else {
        match crate::lightweight::create_main_window(app) {
            Ok(window) => {
                // 在窗口首次显示前同步装饰状态，避免前端加载后再切换导致标题栏闪烁
                // Linux：由设置决定（解决 Wayland 下系统窗口按钮不可用的问题）
                #[cfg(target_os = "linux")]
                let _ = window.set_decorations(!settings.use_app_window_controls);
                // Windows：一律去掉系统标题栏，用页头里的应用内窗口按钮；保留阴影和边缘缩放
                #[cfg(target_os = "windows")]
                {
                    let _ = window.set_decorations(false);
                    let _ = window.set_shadow(true);
                }
                // window-state 插件恢复的位置可能压在任务栏 / Dock 下面，首次显示前收回工作区。
                // 放在装饰设置之后，按最终的窗口外框计算。
                window_fit::fit_to_work_area(&window);

                if visibility == crate::lifecycle::StartupVisibility::Silent {
                    // 静默启动模式：保持窗口隐藏
                    if window.hide().is_ok() {
                        let _ = window.emit("main-window-visibility-changed", ());
                    }
                    #[cfg(target_os = "windows")]
                    let _ = window.set_skip_taskbar(true);
                    #[cfg(target_os = "macos")]
                    tray::apply_tray_policy(app, false);
                    log::info!("静默启动模式：主窗口已隐藏");
                } else {
                    // 正常启动模式：显示窗口
                    if window.show().is_ok() {
                        let _ = window.emit("main-window-visibility-changed", ());
                    }

                    #[cfg(not(target_os = "windows"))]
                    log::info!("正常启动模式：主窗口已显示");

                    // Linux: 解决首次启动 UI 无响应问题（Tauri #10746 + wry #637）。
                    // 启动时 webview 未获取焦点 + surface 尺寸协商失败，导致点击无效。
                    // 这里做 set_focus + 伪 resize，等价于无视觉版本的"最大化-还原"。
                    #[cfg(target_os = "linux")]
                    {
                        linux_fix::nudge_main_window(window.clone(), "startup");
                    }
                }
            }
            Err(error) => log::error!("创建主窗口失败: {error}"),
        }
    }
    Ok(())
}

/// 退出轻量模式且尚未执行过启动会话同步时，在后台补跑一次。
#[cfg(feature = "session-monitor")]
pub(crate) fn trigger_session_sync_if_needed(app: &tauri::AppHandle) {
    if crate::services::session_usage::last_sync_completed_at().is_some() {
        return;
    }
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let db = state.db.clone();
    tauri::async_runtime::spawn(async move {
        run_session_sync_once(db, true).await;
    });
}

#[cfg(not(feature = "session-monitor"))]
pub(crate) fn trigger_session_sync_if_needed(_app: &tauri::AppHandle) {}

#[cfg(feature = "session-monitor")]
async fn run_session_sync_once(db: std::sync::Arc<crate::database::Database>, backfill: bool) {
    // 轻量模式下不后台扫全部客户端会话文件，等退出轻量模式或用户打开用量页时再同步。
    if crate::lightweight::is_lightweight_mode() {
        return;
    }
    // 手动扫描模式下跳过定时扫描；backfill 轮（启动首轮）仍进入，
    // 费用回填只修补数据库既有行（含代理记账行），不读会话文件
    if !backfill && !crate::settings::get_settings().session_auto_sync_enabled {
        return;
    }
    let _guard = crate::services::session_usage::session_sync_mutex()
        .lock()
        .await;
    let db_for_task = db.clone();
    let task = tauri::async_runtime::spawn_blocking(move || {
        if backfill {
            if let Err(error) = db_for_task.backfill_missing_usage_costs() {
                log::warn!("Usage cost startup backfill failed: {error}");
            }
        }
        if !crate::settings::get_settings().session_auto_sync_enabled {
            return crate::services::session_usage::SessionSyncResult::default();
        }
        crate::services::session_usage::sync_all_unlocked(&db_for_task)
    });
    match task.await {
        Ok(result) if !result.errors.is_empty() => {
            log::warn!(
                "Session usage sync completed with {} error(s)",
                result.errors.len()
            );
        }
        Ok(_) => {}
        Err(error) => log::warn!("Session usage blocking task failed: {error}"),
    }
    let _ = db.shrink_memory();
    crate::trim_process_memory();
}

/// 非首屏必需的同步、历史扫描与维护，只在初始化成功之后启动。
fn start_background_jobs(app: &tauri::AppHandle) {
    let state = app.state::<AppState>();
    let event_sink = std::sync::Arc::new(frontend_hooks::TauriEventSink(app.clone()));
    crate::services::auto_sync::start_worker::<crate::services::webdav_auto_sync::WebDavAutoSync>(
        state.db.clone(),
        event_sink.clone(),
    );
    crate::services::auto_sync::start_worker::<crate::services::s3_auto_sync::S3AutoSync>(
        state.db.clone(),
        event_sink,
    );
    // Codex 官方做路由、发布了 Stack 模型时，官方模型列表过期就在后台刷新。
    crate::services::provider::codex_official_models::start_background_checks(
        state.inner().clone(),
    );

    // Codex 历史归桶：setup 同步导入已结束，这里再后台跑，避免堵住
    // mark_startup_settled / 会话同步。函数内部自带完成标记与重试。
    {
        let db_for_codex_history_migration = state.db.clone();
        tauri::async_runtime::spawn_blocking(move || {
            run_codex_history_migrations_on_startup(&db_for_codex_history_migration);
            let _ = db_for_codex_history_migration.shrink_memory();
            crate::trim_process_memory();
        });
    }

    // Backup and log maintenance can hold the database lock for a
    // long time. Keep disk work off both the UI and async workers.
    // The first interval tick runs startup maintenance; later ticks
    // run daily, without a catch-up burst after sleep.
    let db_for_timer = state.db.clone();
    tauri::async_runtime::spawn(async move {
        const PERIODIC_MAINTENANCE_INTERVAL_SECS: u64 = 24 * 60 * 60;
        let mut interval = tokio::time::interval(std::time::Duration::from_secs(
            PERIODIC_MAINTENANCE_INTERVAL_SECS,
        ));
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
        loop {
            interval.tick().await;
            let db = db_for_timer.clone();
            let db_for_trim = db.clone();
            match tauri::async_runtime::spawn_blocking(move || db.periodic_backup_if_needed()).await
            {
                Ok(Ok(())) => {}
                Ok(Err(e)) => log::warn!("Periodic maintenance failed: {e}"),
                Err(e) => log::warn!("Periodic maintenance task failed: {e}"),
            }
            let _ = db_for_trim.shrink_memory();
            crate::trim_process_memory();
        }
    });

    // 会话聚合受 session-monitor 门控：关掉时（CLI 式构建）不启这个任务。
    #[cfg(feature = "session-monitor")]
    {
        // Session log usage sync: 避开首屏峰值延迟 20 秒同步一次，之后每 10 分钟兜底一次；
        // 轻量模式下跳过扫描，用量页打开时仍可按需触发。
        let db_for_session_sync = state.db.clone();
        tauri::async_runtime::spawn(async move {
            const STARTUP_SESSION_SYNC_DELAY_SECS: u64 = 20;
            const SESSION_SYNC_INTERVAL_SECS: u64 = 10 * 60;

            tokio::time::sleep(std::time::Duration::from_secs(
                STARTUP_SESSION_SYNC_DELAY_SECS,
            ))
            .await;
            run_session_sync_once(db_for_session_sync.clone(), true).await;

            let mut interval =
                tokio::time::interval(std::time::Duration::from_secs(SESSION_SYNC_INTERVAL_SECS));
            interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
            interval.tick().await; // skip immediate first tick
            loop {
                interval.tick().await;
                let backfill = crate::services::session_usage::last_sync_completed_at().is_none();
                run_session_sync_once(db_for_session_sync.clone(), backfill).await;
            }
        });
    }
}
