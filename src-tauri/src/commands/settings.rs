#![allow(non_snake_case)]

use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_updater::UpdaterExt;

/// 应用更新下载进度（通过 `update-download-progress` 事件发给前端）。
#[derive(Clone, serde::Serialize)]
struct UpdateDownloadProgress {
    downloaded: u64,
    total: Option<u64>,
}

fn app_updater_builder<R: tauri::Runtime>(
    app: &AppHandle<R>,
) -> Result<tauri_plugin_updater::UpdaterBuilder, String> {
    let status = app
        .try_state::<crate::init_status::UpdaterStatus>()
        .ok_or_else(|| "应用更新插件尚未初始化".to_string())?;
    status.inner().0.as_ref().map_err(Clone::clone)?;
    Ok(app
        .updater_builder()
        .timeout(std::time::Duration::from_secs(30)))
}

fn merge_settings_for_save(
    mut incoming: crate::settings::AppSettings,
    existing: &crate::settings::AppSettings,
) -> crate::settings::AppSettings {
    // Cloud configuration has dedicated commands with credential scoping and
    // the global sync lock. Generic form saves must not replay stale cloud data.
    incoming.webdav_sync = existing.webdav_sync.clone();
    incoming.s3_sync = existing.s3_sync.clone();
    incoming.webdav_backup = existing.webdav_backup.clone();
    // local_migrations 是纯后端状态（迁移完成标记），前端没有合法的修改场景，
    // 无条件取现有值。若按 incoming 透传：后端清掉 marker（如关闭统一会话
    // 开关）后、前端 query 缓存刷新前的一次全量保存会把旧 marker 重放回来，
    // 重新开启时被"复活"的标记挡住而漏迁。
    incoming.local_migrations = existing.local_migrations.clone();
    // 客户端地址只经 set_proxy_client_host 保存（它还要重写客户端）；全量保存带的是前端
    // 缓存里的旧值，透传会把刚保存的地址冲掉。
    incoming.proxy_client_host = existing.proxy_client_host.clone();
    incoming.proxy_lan_token = existing.proxy_lan_token.clone();
    // 轻量模式状态只由进入 / 退出轻量模式时写，前端缓存里的值总是旧的。
    incoming.lightweight_mode = existing.lightweight_mode;
    incoming
}

#[tauri::command]
pub async fn get_settings() -> Result<crate::settings::AppSettings, String> {
    Ok(crate::settings::get_settings_for_frontend())
}

#[derive(Default)]
struct CodexSettingsEffects {
    stack: bool,
    preserve_login: bool,
    unify_history: bool,
}

impl CodexSettingsEffects {
    fn between(
        previous: &crate::settings::AppSettings,
        next: &crate::settings::AppSettings,
    ) -> Self {
        Self {
            stack: next.codex_stack_classic_tools != previous.codex_stack_classic_tools
                || next.codex_stack_subagent_model != previous.codex_stack_subagent_model
                || next.codex_stack_subagent_reasoning_effort
                    != previous.codex_stack_subagent_reasoning_effort
                || next.codex_stack_auto_review_model != previous.codex_stack_auto_review_model,
            preserve_login: next.preserve_codex_official_auth_on_switch
                != previous.preserve_codex_official_auth_on_switch,
            unify_history: next.unify_codex_session_history != previous.unify_codex_session_history,
        }
    }

    async fn apply(&self, state: &crate::store::AppState) -> Result<(), String> {
        let mut failures = Vec::new();
        if self.stack {
            if let Err(error) = crate::mode::controller::resync_codex_stack_catalog(state).await {
                failures.push(format!("Codex Stack: {error}"));
            }
        }
        if self.preserve_login {
            if let Err(error) =
                crate::services::provider::reapply_current_codex_third_party_live(state)
            {
                failures.push(format!("Codex official login: {error}"));
            }
        }
        if self.unify_history {
            if let Err(error) =
                crate::services::provider::reapply_current_codex_official_live(state)
            {
                failures.push(format!("Codex unified history: {error}"));
            }
        }
        if failures.is_empty() {
            Ok(())
        } else {
            Err(failures.join("; "))
        }
    }
}

#[tauri::command]
pub async fn save_settings(
    app: tauri::AppHandle,
    state: tauri::State<'_, crate::store::AppState>,
    settings: serde_json::Value,
) -> Result<bool, String> {
    static SAVE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());
    let _save_guard = SAVE_LOCK.lock().await;
    let (existing, merged) = crate::settings::update_settings_with(|existing| {
        let patch = settings.as_object().ok_or_else(|| {
            crate::error::AppError::InvalidInput("Settings must be an object".into())
        })?;
        let mut value = serde_json::to_value(existing)
            .map_err(|source| crate::error::AppError::JsonSerialize { source })?;
        value.as_object_mut().unwrap().extend(patch.clone());
        let incoming = serde_json::from_value(value)
            .map_err(|error| crate::error::AppError::Config(error.to_string()))?;
        Ok(merge_settings_for_save(incoming, existing))
    })
    .map_err(|error| error.to_string())?;
    let effects = CodexSettingsEffects::between(&existing, &merged);
    let unify_codex_enabled = merged.unify_codex_session_history;
    let tray_style_changed = merged.native_tray_menu != existing.native_tray_menu;

    if let Err(error) = effects.apply(state.inner()).await {
        let mut failures = vec![format!(
            "设置未生效 (Settings could not be applied): {error}"
        )];
        if let Err(rollback_error) = crate::settings::rollback_settings_changes(&existing, &merged)
        {
            failures.push(format!(
                "设置回滚失败 (Settings rollback failed): {rollback_error}"
            ));
        }
        if let Err(recovery_error) = effects.apply(state.inner()).await {
            failures.push(format!(
                "客户端配置恢复失败 (Live configuration recovery failed): {recovery_error}"
            ));
        }
        if tray_style_changed {
            crate::tray::apply_tray_style(&app);
        }
        return Err(failures.join("; "));
    }

    if tray_style_changed {
        crate::tray::apply_tray_style(&app);
    }
    if effects.unify_history {
        if unify_codex_enabled {
            // 后台执行存量迁移（openai 桶 → custom 桶；仅当用户勾选了迁入既有
            // 会话，函数内部自门控）。大会话目录可能要读数秒，不能阻塞设置保存；
            // 失败时不写完成标记，下次启动自动重试。
            tauri::async_runtime::spawn_blocking(|| {
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
            });
        } else {
            // 清除标记与迁移意愿，让重新开启并再次勾选时能补迁
            // 关闭期间落入 openai 桶的官方会话。
            if let Err(err) = crate::settings::clear_codex_official_history_unify_migration() {
                log::warn!("清除统一会话迁移标记失败: {err}");
            }
            if let Err(err) = crate::settings::clear_codex_unify_migrate_existing() {
                log::warn!("清除统一会话迁移意愿失败: {err}");
            }
        }
    }
    Ok(true)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CodexUnifyHistoryRestoreResult {
    pub restored_jsonl_files: usize,
    pub restored_state_rows: usize,
    /// 还原被跳过的原因（如当前目录没有账本），前端据此提示而非报"成功 0 项"。
    #[serde(skip_serializing_if = "Option::is_none")]
    pub skipped_reason: Option<String>,
}

/// 是否存在统一会话开关的迁移备份（决定关闭弹窗里是否显示"恢复备份"勾选）。
#[tauri::command]
pub async fn has_codex_unify_history_backup() -> Result<bool, String> {
    Ok(crate::codex_history_migration::has_codex_official_history_unify_backup())
}

/// Codex 的 `config.toml` 是不是用 `[features] multi_agent_v2` 强制了新版子 agent 工具
/// （这时「经典子 agent 工具」开关不生效，界面上提示）。
#[tauri::command]
pub async fn codex_forces_multi_agent_v2() -> Result<bool, String> {
    let text = crate::codex_config::read_codex_config_text().map_err(|e| e.to_string())?;
    Ok(crate::codex_config::codex_config_forces_multi_agent_v2(
        &text,
    ))
}

/// Codex 会话压缩开关（`[features] local_thread_store_compression`）当前是否开启。
#[tauri::command]
pub async fn get_codex_session_compression() -> Result<bool, String> {
    crate::services::codex_session_compression::is_enabled().map_err(|e| e.to_string())
}

/// 开关 Codex 会话压缩，返回写入后的状态。
#[tauri::command]
pub async fn set_codex_session_compression(
    state: tauri::State<'_, crate::store::AppState>,
    enabled: bool,
) -> Result<bool, String> {
    let db = state.db.clone();
    tauri::async_runtime::spawn_blocking(move || {
        crate::services::codex_session_compression::set_enabled(&db, enabled)
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// Codex 会话目录（sessions + archived_sessions）当前占用的字节数。
#[cfg(feature = "session-monitor")]
#[tauri::command]
pub async fn get_codex_sessions_disk_usage() -> Result<u64, String> {
    tauri::async_runtime::spawn_blocking(
        crate::services::codex_session_compression::sessions_disk_usage,
    )
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())
}

/// 按迁移备份账本把当时迁入共享桶的官方会话还原回 "openai" 桶。
/// 由关闭统一会话开关的确认弹窗触发；幂等，可安全重试。
#[tauri::command]
pub async fn restore_codex_unified_history() -> Result<CodexUnifyHistoryRestoreResult, String> {
    let outcome = tauri::async_runtime::spawn_blocking(|| {
        crate::codex_history_migration::restore_codex_official_history_from_backups()
    })
    .await
    .map_err(|e| e.to_string())?
    .map_err(|e| e.to_string())?;

    if let Some(reason) = &outcome.skipped_reason {
        log::debug!("○ Codex official history restore skipped: {reason}");
    } else {
        log::info!(
            "✓ Codex official history restored from backups: jsonl_files={}, state_rows={}",
            outcome.restored_jsonl_files,
            outcome.restored_state_rows
        );
    }

    Ok(CodexUnifyHistoryRestoreResult {
        restored_jsonl_files: outcome.restored_jsonl_files,
        restored_state_rows: outcome.restored_state_rows,
        skipped_reason: outcome.skipped_reason,
    })
}

// 更新安装与目录重启必须互斥，窗口重建不能绕过此锁。
static RESTART_UPDATE_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

/// 重启应用程序（当 app_config_dir 变更后使用）
#[tauri::command]
pub async fn restart_app(app: AppHandle) -> Result<bool, String> {
    let operation = RESTART_UPDATE_LOCK
        .try_lock()
        .map_err(|_| "应用正在更新或重启，请等待当前操作完成".to_string())?;
    crate::save_window_state_before_exit(&app);

    // 在后台延迟重启，让函数有时间返回响应
    tauri::async_runtime::spawn(async move {
        let _operation = operation;
        tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
        // 本命令用于 app_config_dir 变更后的重启：新实例会切到新数据库，拿不到
        // 旧库里的 Live 备份，无法恢复被接管的 Live 配置。因此必须趁旧实例的
        // 事件循环仍存活，在这里同步完成恢复（保留代理状态，新实例启动时自动
        // 重新接管）。
        crate::run_exit_cleanup_once(&app).await;
        crate::wait_for_db_quiescent(&app).await;
        // 与自更新共用 restart_process，而不是 app.restart()：macOS 上经
        // LaunchServices 启动，新窗口才能回到前台。
        crate::restart_process(&app);
    });
    Ok(true)
}

/// 下载并安装应用更新，然后由后端直接重启应用。
///
/// macOS 更新会原地替换 `.app` bundle。如果先返回前端、再让旧 WebView 调
/// `process.relaunch()`，旧进程可能已经处在 bundle 被替换后的不稳定窗口期。
/// 这里把退出清理、安装和重启串在同一个后端流程中，避免依赖旧前端继续执行。
#[tauri::command]
pub async fn install_update_and_restart(app: AppHandle) -> Result<bool, String> {
    let operation = RESTART_UPDATE_LOCK
        .try_lock()
        .map_err(|_| "应用正在更新或重启，请等待当前操作完成".to_string())?;
    let builder = app_updater_builder(&app)?;
    // Windows 上 updater 以 `msiexec /passive /promptrestart` 安装 MSI。passive 没有模态框、
    // 未指定重启选项时，安装目录里有文件被占用，Windows Installer 会不经确认直接重启系统
    // （#8534）。`/promptrestart` 只对应 REBOOTPROMPT，挡不住这一点，因此显式追加
    // REBOOT=ReallySuppress（即 `/norestart`）。旧版本的 updater 不带这个参数，
    // 由 MSI 模板自身的 REBOOT 属性兜底。
    #[cfg(target_os = "windows")]
    let builder = builder
        .installer_arg("REBOOT=ReallySuppress")
        .on_before_exit({
            let app = app.clone();
            move || {
                // 插件已成功解包，即将启动安装器并退出；此前的失败仍可继续使用应用。
                crate::save_window_state_before_exit(&app);
                tauri::async_runtime::block_on(crate::run_exit_cleanup_once(&app));
                crate::remove_tray_icon_before_exit(&app);
                crate::destroy_single_instance_lock(&app);
                tauri::async_runtime::block_on(crate::wait_for_db_quiescent(&app));
                // 自定义 hook 替换了插件默认 hook，保留 Tauri 自身的资源清理。
                app.cleanup_before_exit();
            }
        });
    let updater = builder
        .build()
        .map_err(|e| format!("初始化更新器失败: {e}"))?;

    let Some(update) = updater
        .check()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?
    else {
        return Ok(false);
    };

    log::info!("开始下载应用更新: {}", update.version);
    let progress_handle = app.clone();
    let mut downloaded: u64 = 0;
    let bytes = update
        .download(
            move |chunk_len, content_len| {
                downloaded = downloaded.saturating_add(chunk_len as u64);
                let _ = progress_handle.emit(
                    "update-download-progress",
                    UpdateDownloadProgress {
                        downloaded,
                        total: content_len,
                    },
                );
            },
            || {},
        )
        .await
        .map_err(|e| format!("下载更新失败: {e}"))?;

    log::info!("开始安装应用更新: {}", update.version);

    // 所有平台的安装都是同步磁盘操作。锁随 worker 移动，IPC 调用方
    // 取消等待也不能提前放锁，让第二个安装器与仍在运行的第一个交错。
    tauri::async_runtime::spawn_blocking(move || -> Result<bool, String> {
        let _operation = operation;
        update
            .install(bytes)
            .map_err(|e| format!("安装更新失败: {e}"))?;

        #[cfg(not(target_os = "windows"))]
        {
            // 只有安装成功才消费一次性退出清理，失败后仍可继续使用应用。
            crate::save_window_state_before_exit(&app);
            tauri::async_runtime::block_on(async {
                crate::run_exit_cleanup_once(&app).await;
                crate::wait_for_db_quiescent(&app).await;
            });
            log::info!("应用更新安装完成，正在重启应用");
            crate::restart_process(&app);
        }
        #[cfg(target_os = "windows")]
        Ok(true)
    })
    .await
    .map_err(|e| format!("更新安装任务失败: {e}"))?
}

/// 检查是否有可用的应用更新，返回可用的新版本号（无更新时返回 None）。
///
/// 数据库版本过新的恢复界面用它判断：升级应用能否解决问题。若返回 None，说明
/// 已是最新版本，但数据库仍不兼容（通常由第三方客户端或更高版本创建），应提示用户
/// 升级无法解决，而不是让其反复尝试。
#[tauri::command]
pub async fn check_app_update_available(app: AppHandle) -> Result<Option<String>, String> {
    let updater = app_updater_builder(&app)?
        .build()
        .map_err(|e| format!("初始化更新器失败: {e}"))?;
    let update = updater
        .check()
        .await
        .map_err(|e| format!("检查更新失败: {e}"))?;
    Ok(update.map(|u| u.version))
}

/// 获取 app_config_dir 覆盖配置 (从 Store)
///
/// 只回显 Store 里的值，不改变本进程生效目录：设置页可能刚保存了新目录但
/// 选了「稍后重启」，这里若顺手切换，数据库就会和配置文件分叉。
#[tauri::command]
pub async fn get_app_config_dir_override(app: AppHandle) -> Result<Option<String>, String> {
    Ok(crate::app_store::read_app_config_dir_override(&app)
        .map(|p| p.to_string_lossy().to_string()))
}

/// 设置 app_config_dir 覆盖配置 (到 Store)
#[tauri::command]
pub async fn set_app_config_dir_override(
    app: AppHandle,
    path: Option<String>,
) -> Result<bool, String> {
    crate::app_store::set_app_config_dir_to_store(&app, path.as_deref())?;
    Ok(true)
}

#[tauri::command]
pub async fn set_auto_launch(enabled: bool) -> Result<bool, String> {
    if enabled {
        crate::auto_launch::enable_auto_launch().map_err(|e| format!("启用开机自启失败: {e}"))?;
    } else {
        crate::auto_launch::disable_auto_launch().map_err(|e| format!("禁用开机自启失败: {e}"))?;
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::merge_settings_for_save;

    #[test]
    fn unavailable_updater_returns_error_without_accessing_plugin_state() {
        use tauri::Manager;
        let app = tauri::test::mock_builder()
            .build(tauri::test::mock_context(tauri::test::noop_assets()))
            .unwrap();
        assert_eq!(
            super::app_updater_builder(app.handle()).err().as_deref(),
            Some("应用更新插件尚未初始化")
        );
        app.manage(crate::init_status::UpdaterStatus(Err(
            "invalid updater config".into(),
        )));
        assert_eq!(
            super::app_updater_builder(app.handle()).err().as_deref(),
            Some("invalid updater config")
        );
    }

    #[test]
    fn combined_settings_patch_plans_all_affected_live_updates() {
        let existing = crate::settings::AppSettings::default();
        let mut changed = existing.clone();
        changed.codex_stack_classic_tools = !existing.codex_stack_classic_tools;
        changed.preserve_codex_official_auth_on_switch =
            !existing.preserve_codex_official_auth_on_switch;
        changed.unify_codex_session_history = !existing.unify_codex_session_history;
        let effects = super::CodexSettingsEffects::between(&existing, &changed);
        assert!(effects.stack && effects.preserve_login && effects.unify_history);
        let unchanged = super::CodexSettingsEffects::between(&changed, &changed);
        assert!(!unchanged.stack && !unchanged.preserve_login && !unchanged.unify_history);
    }
    use crate::settings::{
        AppSettings, CodexOfficialHistoryUnifyMigration, CodexProviderTemplateMigration,
        CodexThirdPartyHistoryProviderBucketMigration, LocalMigrations, S3SyncSettings,
        WebDavSyncSettings,
    };

    #[test]
    fn save_settings_should_keep_the_existing_lightweight_mode() {
        let existing = AppSettings {
            lightweight_mode: true,
            ..AppSettings::default()
        };

        let merged = merge_settings_for_save(AppSettings::default(), &existing);

        assert!(merged.lightweight_mode);
    }

    #[test]
    fn save_settings_should_keep_the_existing_proxy_client_host() {
        let existing = AppSettings {
            proxy_client_host: Some("172.25.144.1".to_string()),
            ..AppSettings::default()
        };

        let merged = merge_settings_for_save(AppSettings::default(), &existing);

        assert_eq!(merged.proxy_client_host.as_deref(), Some("172.25.144.1"));
    }

    #[test]
    fn save_settings_should_preserve_existing_webdav_when_payload_omits_it() {
        let existing = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.example.com".to_string(),
                username: "alice".to_string(),
                password: "secret".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings::default();
        let merged = merge_settings_for_save(incoming, &existing);

        assert!(merged.webdav_sync.is_some());
        assert_eq!(
            merged.webdav_sync.as_ref().map(|v| v.base_url.as_str()),
            Some("https://dav.example.com")
        );
    }

    #[test]
    fn generic_save_preserves_cloud_configuration_even_when_payload_contains_it() {
        let existing = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.old.example.com".to_string(),
                username: "old".to_string(),
                password: "old-pass".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.new.example.com".to_string(),
                username: "new".to_string(),
                password: "new-pass".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let merged = merge_settings_for_save(incoming, &existing);

        assert_eq!(
            merged.webdav_sync.as_ref().map(|v| v.base_url.as_str()),
            Some("https://dav.old.example.com")
        );
    }

    /// Regression test: frontend always receives empty password from
    /// get_settings_for_frontend(). If a component accidentally spreads
    /// the full settings object into save_settings, the empty password
    /// must NOT overwrite the existing one.
    #[test]
    fn save_settings_should_preserve_password_when_incoming_has_empty_password() {
        let existing = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.example.com".to_string(),
                username: "alice".to_string(),
                password: "secret".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        // Simulate frontend sending settings with cleared password
        let incoming = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.example.com".to_string(),
                username: "alice".to_string(),
                password: "".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let merged = merge_settings_for_save(incoming, &existing);

        assert_eq!(
            merged.webdav_sync.as_ref().map(|v| v.password.as_str()),
            Some("secret"),
            "empty password from frontend must not overwrite existing password"
        );
    }

    /// When both incoming and existing have no password, merge should
    /// work without panicking and keep the empty state.
    #[test]
    fn save_settings_should_handle_both_empty_passwords() {
        let existing = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.example.com".to_string(),
                username: "alice".to_string(),
                password: "".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings {
            webdav_sync: Some(WebDavSyncSettings {
                base_url: "https://dav.example.com".to_string(),
                username: "alice".to_string(),
                password: "".to_string(),
                ..WebDavSyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let merged = merge_settings_for_save(incoming, &existing);

        assert_eq!(
            merged.webdav_sync.as_ref().map(|v| v.password.as_str()),
            Some("")
        );
    }

    #[test]
    fn save_settings_should_preserve_existing_s3_when_payload_omits_it() {
        let existing = AppSettings {
            s3_sync: Some(S3SyncSettings {
                bucket: "bucket".to_string(),
                access_key_id: "ak".to_string(),
                secret_access_key: "secret".to_string(),
                ..S3SyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings::default();
        let merged = merge_settings_for_save(incoming, &existing);

        assert!(merged.s3_sync.is_some());
        assert_eq!(
            merged
                .s3_sync
                .as_ref()
                .map(|v| v.secret_access_key.as_str()),
            Some("secret")
        );
    }

    #[test]
    fn save_settings_should_preserve_s3_secret_when_incoming_has_empty_secret() {
        let existing = AppSettings {
            s3_sync: Some(S3SyncSettings {
                bucket: "bucket".to_string(),
                access_key_id: "ak".to_string(),
                secret_access_key: "secret".to_string(),
                ..S3SyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings {
            s3_sync: Some(S3SyncSettings {
                bucket: "bucket".to_string(),
                access_key_id: "ak".to_string(),
                secret_access_key: "".to_string(),
                ..S3SyncSettings::default()
            }),
            ..AppSettings::default()
        };

        let merged = merge_settings_for_save(incoming, &existing);

        assert_eq!(
            merged
                .s3_sync
                .as_ref()
                .map(|v| v.secret_access_key.as_str()),
            Some("secret")
        );
    }

    #[test]
    fn save_settings_should_preserve_local_migrations_when_payload_omits_it() {
        let existing = AppSettings {
            local_migrations: Some(LocalMigrations {
                codex_third_party_history_provider_bucket_v1: Some(
                    CodexThirdPartyHistoryProviderBucketMigration {
                        completed_at: "2026-05-20T00:00:00Z".to_string(),
                        target_provider_id: "custom".to_string(),
                        source_provider_ids: vec!["rightcode".to_string()],
                        migrated_jsonl_files: 2,
                        migrated_state_rows: 3,
                        scanned_history_files: true,
                    },
                ),
                codex_provider_template_v1: Some(CodexProviderTemplateMigration {
                    completed_at: "2026-05-20T00:01:00Z".to_string(),
                    migrated_provider_ids: vec!["legacy".to_string()],
                }),
                codex_official_history_unify_v1: Some(CodexOfficialHistoryUnifyMigration {
                    completed_at: "2026-06-12T00:00:00Z".to_string(),
                    target_provider_id: "custom".to_string(),
                    migrated_jsonl_files: 5,
                    migrated_state_rows: 7,
                    codex_config_dir: None,
                }),
                ..LocalMigrations::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings::default();
        let merged = merge_settings_for_save(incoming, &existing);

        let migration = merged
            .local_migrations
            .as_ref()
            .and_then(|migrations| {
                migrations
                    .codex_third_party_history_provider_bucket_v1
                    .as_ref()
            })
            .expect("local migration marker should be preserved");
        assert_eq!(migration.target_provider_id, "custom");
        assert_eq!(migration.migrated_jsonl_files, 2);
        assert_eq!(migration.migrated_state_rows, 3);

        let template_migration = merged
            .local_migrations
            .as_ref()
            .and_then(|migrations| migrations.codex_provider_template_v1.as_ref())
            .expect("template migration marker should be preserved");
        assert_eq!(
            template_migration.migrated_provider_ids,
            vec!["legacy".to_string()]
        );

        let unify_migration = merged
            .local_migrations
            .as_ref()
            .and_then(|migrations| migrations.codex_official_history_unify_v1.as_ref())
            .expect("official unify migration marker should be preserved");
        assert_eq!(unify_migration.migrated_jsonl_files, 5);
        assert_eq!(unify_migration.migrated_state_rows, 7);
    }

    /// incoming 带有 local_migrations（哪怕是空的）也不能覆盖后端维护的标记。
    #[test]
    fn save_settings_should_keep_backend_migration_markers_over_incoming() {
        let existing = AppSettings {
            local_migrations: Some(LocalMigrations {
                codex_third_party_history_provider_bucket_v1: None,
                codex_provider_template_v1: None,
                codex_official_history_unify_v1: Some(CodexOfficialHistoryUnifyMigration {
                    completed_at: "2026-06-12T00:00:00Z".to_string(),
                    target_provider_id: "custom".to_string(),
                    migrated_jsonl_files: 1,
                    migrated_state_rows: 2,
                    codex_config_dir: None,
                }),
                ..LocalMigrations::default()
            }),
            ..AppSettings::default()
        };

        let incoming = AppSettings {
            local_migrations: Some(LocalMigrations::default()),
            ..AppSettings::default()
        };
        let merged = merge_settings_for_save(incoming, &existing);

        assert!(merged
            .local_migrations
            .as_ref()
            .and_then(|migrations| migrations.codex_official_history_unify_v1.as_ref())
            .is_some());
    }

    /// 后端清掉 marker 后（如关闭统一会话开关）、前端缓存刷新前的全量保存
    /// 会携带旧 marker；merge 必须忽略它，否则被"复活"的标记会让重新开启
    /// 时误判已迁移而漏迁。
    #[test]
    fn save_settings_should_ignore_stale_incoming_migration_markers() {
        let existing = AppSettings::default();

        let incoming = AppSettings {
            local_migrations: Some(LocalMigrations {
                codex_official_history_unify_v1: Some(CodexOfficialHistoryUnifyMigration {
                    completed_at: "2026-06-12T00:00:00Z".to_string(),
                    target_provider_id: "custom".to_string(),
                    migrated_jsonl_files: 1,
                    migrated_state_rows: 2,
                    codex_config_dir: None,
                }),
                ..LocalMigrations::default()
            }),
            ..AppSettings::default()
        };
        let merged = merge_settings_for_save(incoming, &existing);

        assert!(merged.local_migrations.is_none());
    }
}

#[tauri::command]
pub async fn get_auto_launch_status() -> Result<bool, String> {
    crate::auto_launch::is_auto_launch_enabled().map_err(|e| format!("获取开机自启状态失败: {e}"))
}

#[tauri::command]
pub async fn get_rectifier_config(
    state: tauri::State<'_, crate::AppState>,
) -> Result<crate::proxy::types::RectifierConfig, String> {
    state.db.get_rectifier_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_rectifier_config(
    state: tauri::State<'_, crate::AppState>,
    config: crate::proxy::types::RectifierConfig,
) -> Result<bool, String> {
    state
        .db
        .set_rectifier_config(&config)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn get_optimizer_config(
    state: tauri::State<'_, crate::AppState>,
) -> Result<crate::proxy::types::OptimizerConfig, String> {
    state.db.get_optimizer_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_optimizer_config(
    state: tauri::State<'_, crate::AppState>,
    config: crate::proxy::types::OptimizerConfig,
) -> Result<bool, String> {
    state
        .db
        .set_optimizer_config(&config)
        .map_err(|e| e.to_string())?;
    Ok(true)
}

#[tauri::command]
pub async fn get_log_config(
    state: tauri::State<'_, crate::AppState>,
) -> Result<crate::proxy::types::LogConfig, String> {
    state.db.get_log_config().map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn set_log_config(
    state: tauri::State<'_, crate::AppState>,
    config: crate::proxy::types::LogConfig,
) -> Result<bool, String> {
    state
        .db
        .set_log_config(&config)
        .map_err(|e| e.to_string())?;
    log::set_max_level(config.to_level_filter());
    log::info!(
        "日志配置已更新: enabled={}, level={}",
        config.enabled,
        config.level
    );
    Ok(true)
}
