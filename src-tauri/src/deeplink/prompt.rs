//! Prompt import from deep link
//!
//! Handles importing prompt configurations via ccswitch:// URLs.

use super::utils::decode_base64_param;
use super::DeepLinkImportRequest;
use crate::error::AppError;
use crate::prompt::Prompt;
use crate::services::PromptService;
use crate::store::AppState;
use crate::AppType;
use std::str::FromStr;

/// Import a prompt from deep link request
pub fn import_prompt_from_deeplink(
    state: &AppState,
    request: DeepLinkImportRequest,
) -> Result<String, AppError> {
    // Verify this is a prompt request
    if request.resource != "prompt" {
        return Err(AppError::InvalidInput(format!(
            "Expected prompt resource, got '{}'",
            request.resource
        )));
    }

    // Extract required fields
    let app_str = request
        .app
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("Missing 'app' field for prompt".to_string()))?;

    let name = request
        .name
        .ok_or_else(|| AppError::InvalidInput("Missing 'name' field for prompt".to_string()))?;

    // Parse app type
    let app_type = AppType::from_str(app_str)
        .map_err(|_| AppError::InvalidInput(format!("Invalid app type: {app_str}")))?;

    // Decode content
    let content_b64 = request
        .content
        .as_ref()
        .ok_or_else(|| AppError::InvalidInput("Missing 'content' field for prompt".to_string()))?;

    let content = decode_base64_param("content", content_b64)?;
    let content = String::from_utf8(content)
        .map_err(|e| AppError::InvalidInput(format!("Invalid UTF-8 in content: {e}")))?;

    // Generate ID
    let timestamp = chrono::Utc::now().timestamp();
    let sanitized_name = name
        .chars()
        .filter(|c| c.is_alphanumeric() || *c == '-' || *c == '_')
        .collect::<String>()
        .to_lowercase();
    let id = format!("{sanitized_name}-{}", uuid::Uuid::new_v4());

    // Check if we should enable this prompt
    let should_enable = request.enabled.unwrap_or(false);
    let activate_pi = should_enable && app_type == AppType::Pi;

    let prompt = Prompt {
        id: id.clone(),
        name: name.clone(),
        content,
        description: request.description,
        enabled: should_enable && !activate_pi,
        created_at: Some(timestamp),
        updated_at: Some(timestamp),
    };

    // Save using PromptService
    PromptService::upsert_prompt(state, app_type.clone(), &id, prompt)?;

    if activate_pi {
        if let Err(error) = PromptService::enable_prompt(state, app_type.clone(), &id) {
            if let Err(rollback_error) = state.db.delete_prompt(&app_type, &id) {
                return Err(AppError::Message(format!(
                    "{error}; imported prompt cleanup failed: {rollback_error}"
                )));
            }
            return Err(error);
        }
    }

    if should_enable {
        log::info!("Successfully imported and enabled prompt '{name}' for {app_str}");
    } else {
        log::info!("Successfully imported prompt '{name}' for {app_str} (disabled)");
    }

    Ok(id)
}
