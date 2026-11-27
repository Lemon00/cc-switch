use crate::app_config::AppType;
use tauri::State;

use crate::services::omo::{OmoLocalFileData, SLIM, STANDARD};
use crate::services::OmoService;
use crate::store::AppState;

#[tauri::command]
pub async fn read_omo_local_file() -> Result<OmoLocalFileData, String> {
    OmoService::read_local_file(&STANDARD).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_current_omo_provider_id(state: State<'_, AppState>) -> Result<String, String> {
    let provider = state
        .db
        .get_current_omo_provider(&AppType::OpenCode, "omo")
        .map_err(|e| e.to_string())?;
    Ok(provider.map(|p| p.id).unwrap_or_default())
}

#[tauri::command]
pub async fn disable_current_omo(state: State<'_, AppState>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || OmoService::disable_current(&state, &STANDARD))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}

// ── OMO Slim commands ───────────────────────────────────────

#[tauri::command]
pub async fn read_omo_slim_local_file() -> Result<OmoLocalFileData, String> {
    OmoService::read_local_file(&SLIM).map_err(|e| e.to_string())
}

#[tauri::command]
pub async fn get_current_omo_slim_provider_id(
    state: State<'_, AppState>,
) -> Result<String, String> {
    let provider = state
        .db
        .get_current_omo_provider(&AppType::OpenCode, "omo-slim")
        .map_err(|e| e.to_string())?;
    Ok(provider.map(|p| p.id).unwrap_or_default())
}

#[tauri::command]
pub async fn disable_current_omo_slim(state: State<'_, AppState>) -> Result<(), String> {
    let state = state.inner().clone();
    tauri::async_runtime::spawn_blocking(move || OmoService::disable_current(&state, &SLIM))
        .await
        .map_err(|error| error.to_string())?
        .map_err(|error| error.to_string())
}
