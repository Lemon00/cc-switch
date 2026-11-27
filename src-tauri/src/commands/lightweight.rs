#[tauri::command]
pub async fn enter_lightweight_mode(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::lightweight::enter_lightweight_mode(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub async fn exit_lightweight_mode(app: tauri::AppHandle) -> Result<(), String> {
    tauri::async_runtime::spawn_blocking(move || crate::lightweight::exit_lightweight_mode(&app))
        .await
        .map_err(|error| error.to_string())?
}

#[tauri::command]
pub fn is_lightweight_mode() -> bool {
    crate::lightweight::is_lightweight_mode()
}
