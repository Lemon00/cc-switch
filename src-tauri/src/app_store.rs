use serde_json::Value;
use std::path::PathBuf;
use tauri_plugin_store::StoreExt;

use crate::error::AppError;

/// Store 中的键名（与核心共用，CLI 读同一份文件时才不会各写一份）。
const STORE_KEY_APP_CONFIG_DIR: &str = cc_switch_core::config::APP_CONFIG_DIR_STORE_KEY;
/// Store 文件名（同上）。
const STORE_FILE_APP_PATHS: &str = cc_switch_core::config::APP_PATHS_STORE_FILE;

// 覆盖路径只保留一份：核心的 `config::CONFIG_DIR_OVERRIDE`。
// 这里曾有一份自己的 OnceLock 缓存，和核心各存一份 PathBuf，改路径时容易
// 只更新一边（审计 #21）。现在读 Store 只做「回显」，生效值一律问核心。

fn read_override_from_store(app: &tauri::AppHandle) -> Option<PathBuf> {
    let store = match app.store_builder(STORE_FILE_APP_PATHS).build() {
        Ok(store) => store,
        Err(e) => {
            log::warn!("无法创建 Store: {e}");
            return None;
        }
    };

    match store.get(STORE_KEY_APP_CONFIG_DIR) {
        // 去空白 / `~` 展开 / 路径必须存在，这套解析规则在核心，
        // CLI 读同一个文件时用的是同一份实现。
        Some(Value::String(path_str)) => {
            let path = cc_switch_core::config::parse_app_config_dir_override(&path_str)?;
            log::info!("使用 Store 中的 app_config_dir: {path:?}");
            Some(path)
        }
        Some(_) => {
            log::warn!("Store 中的 {STORE_KEY_APP_CONFIG_DIR} 类型不正确，应为字符串");
            None
        }
        None => None,
    }
}

/// 启动阶段用：与 CLI 同一套优先级（`CC_SWITCH_CONFIG_DIR` > Store > 默认），
/// 决定本进程生效的配置目录并写进核心。
///
/// Store 由 GUI 自己读（插件路径由 Tauri 决定），环境变量交给核心解析，
/// 避免两处实现判出不同结果。
///
/// 只在进程启动、打开数据库之前调用。运行中改目录不要再走这里——数据库
/// 还连着旧目录，而新文件已经写到新目录，会读写分叉。
pub fn refresh_app_config_dir_override(app: &tauri::AppHandle) -> Option<PathBuf> {
    let value = cc_switch_core::config::app_config_dir_override_from_env()
        .or_else(|| read_override_from_store(app));
    if let Some(path) = &value {
        cc_switch_core::config::set_app_config_dir_override(path.clone());
        cc_switch_core::settings::AppSettings::migrate_legacy_settings_file_if_needed();
    }
    value
}

/// 运行中读取「重启后会生效的覆盖目录」（设置页回显用，不改变本进程目录）。
///
/// 与启动判定同序：`CC_SWITCH_CONFIG_DIR` 优先，其次 Store。这样设了环境变量时
/// 设置页不会再显示一个重启后不会生效的 Store 值。
pub fn read_app_config_dir_override(app: &tauri::AppHandle) -> Option<PathBuf> {
    cc_switch_core::config::app_config_dir_override_from_env()
        .or_else(|| read_override_from_store(app))
}

/// 写入 app_config_dir 到 Tauri Store
pub fn set_app_config_dir_to_store(
    app: &tauri::AppHandle,
    path: Option<&str>,
) -> Result<(), AppError> {
    let store = app
        .store_builder(STORE_FILE_APP_PATHS)
        .build()
        .map_err(|e| AppError::Message(format!("创建 Store 失败: {e}")))?;

    match path {
        Some(p) => {
            let trimmed = p.trim();
            if !trimmed.is_empty() {
                // 启动期对不存在的目录只会 warn 然后按默认目录跑，所以这里必须
                // 直接拒绝：否则用户看到「已保存」，重启后却悄悄换了目录。
                let resolved = cc_switch_core::config::parse_app_config_dir_override(trimmed)
                    .ok_or_else(|| {
                        AppError::Message(format!(
                            "配置目录不存在或不可用，请选择已存在的目录: {trimmed}"
                        ))
                    })?;
                // 存展开后的绝对路径：`~` 由核心解释，存原文会随进程环境变化。
                let resolved = resolved.to_string_lossy().to_string();
                store.set(STORE_KEY_APP_CONFIG_DIR, Value::String(resolved.clone()));
                log::info!("已将 app_config_dir 写入 Store: {resolved}");
            } else {
                store.delete(STORE_KEY_APP_CONFIG_DIR);
                log::info!("已从 Store 中删除 app_config_dir 配置");
            }
        }
        None => {
            store.delete(STORE_KEY_APP_CONFIG_DIR);
            log::info!("已从 Store 中删除 app_config_dir 配置");
        }
    }

    store
        .save()
        .map_err(|e| AppError::Message(format!("保存 Store 失败: {e}")))?;

    // 只落盘，不在这里改核心的生效目录：设置页允许「稍后重启」，那时数据库
    // 连接还指着旧目录，立刻切换只会让 DB 写旧目录、新文件写新目录。
    // 真正生效统一在下次启动（setup 里的 refresh_app_config_dir_override）。
    Ok(())
}
