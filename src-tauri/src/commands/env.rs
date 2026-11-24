use crate::services::env_checker::{check_env_conflicts as check_conflicts, EnvConflict};
use crate::services::env_manager::{
    delete_env_vars as delete_vars, restore_from_backup, BackupInfo,
};

/// Check environment variable conflicts for a specific app
#[tauri::command]
pub async fn check_env_conflicts(app: String) -> Result<Vec<EnvConflict>, String> {
    tauri::async_runtime::spawn_blocking(move || check_conflicts(&app))
        .await
        .map_err(|error| format!("环境变量检查任务失败: {error}"))?
}

/// Delete environment variables with backup
#[tauri::command]
pub async fn delete_env_vars(conflicts: Vec<EnvConflict>) -> Result<BackupInfo, String> {
    tauri::async_runtime::spawn_blocking(move || delete_vars(conflicts))
        .await
        .map_err(|error| format!("环境变量删除任务失败: {error}"))?
}

/// Restore environment variables from backup file
#[tauri::command]
pub async fn restore_env_backup(backup_path: String) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || restore_from_backup(backup_path))
        .await
        .map_err(|error| format!("环境变量恢复任务失败: {error}"))?
}
