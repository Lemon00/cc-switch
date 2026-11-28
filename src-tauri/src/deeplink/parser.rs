//! Deep link URL parser
//!
//! Parses ccswitch:// URLs into DeepLinkImportRequest structures.

use super::utils::{validate_url, MAX_DEEPLINK_BYTES};
use super::DeepLinkImportRequest;
use crate::error::AppError;
use std::collections::HashMap;
use url::Url;

/// Parse a ccswitch:// URL into a DeepLinkImportRequest
///
/// Expected format:
/// ccswitch://v1/import?resource={type}&...
pub fn parse_deeplink_url(url_str: &str) -> Result<DeepLinkImportRequest, AppError> {
    if url_str.len() > MAX_DEEPLINK_BYTES {
        return Err(AppError::InvalidInput(
            "Deep link exceeds the 256 KiB limit".into(),
        ));
    }
    // Parse URL
    let url = Url::parse(url_str)
        .map_err(|e| AppError::InvalidInput(format!("Invalid deep link URL: {e}")))?;

    // Validate scheme
    let scheme = url.scheme();
    if scheme != "ccswitch" {
        return Err(AppError::InvalidInput(format!(
            "Invalid scheme: expected 'ccswitch', got '{scheme}'"
        )));
    }

    if !url.username().is_empty()
        || url.password().is_some()
        || url.port().is_some()
        || url.fragment().is_some()
    {
        return Err(AppError::InvalidInput(
            "Deep links cannot contain credentials, a port or a fragment".into(),
        ));
    }

    // Extract version from host
    let version = url
        .host_str()
        .ok_or_else(|| AppError::InvalidInput("Missing version in URL host".to_string()))?
        .to_string();

    // Validate version
    if version != "v1" {
        return Err(AppError::InvalidInput(format!(
            "Unsupported protocol version: {version}"
        )));
    }

    // Extract path (should be "/import")
    let path = url.path();
    if path != "/import" {
        return Err(AppError::InvalidInput(format!(
            "Invalid path: expected '/import', got '{path}'"
        )));
    }

    // Parse query parameters
    let mut params = HashMap::new();
    for (key, value) in url.query_pairs() {
        if params
            .insert(key.into_owned(), value.into_owned())
            .is_some()
        {
            return Err(AppError::InvalidInput(
                "Duplicate deep link parameter".into(),
            ));
        }
        if params.len() > 64 {
            return Err(AppError::InvalidInput(
                "Too many deep link parameters".into(),
            ));
        }
    }

    // Extract and validate resource type
    let resource = params
        .get("resource")
        .ok_or_else(|| AppError::InvalidInput("Missing 'resource' parameter".to_string()))?
        .clone();

    // Dispatch to appropriate parser based on resource type
    match resource.as_str() {
        "provider" => parse_provider_deeplink(&params, version, resource),
        "prompt" => parse_prompt_deeplink(&params, version, resource),
        "mcp" => parse_mcp_deeplink(&params, version, resource),
        "skill" => parse_skill_deeplink(&params, version, resource),
        _ => Err(AppError::InvalidInput(format!(
            "Unsupported resource type: {resource}"
        ))),
    }
}

fn optional_bool(params: &HashMap<String, String>, key: &str) -> Result<Option<bool>, AppError> {
    params
        .get(key)
        .map(|value| {
            value
                .parse::<bool>()
                .map_err(|_| AppError::InvalidInput(format!("'{key}' must be true or false")))
        })
        .transpose()
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINK: &str = "ccswitch://v1/import?resource=provider&app=claude&name=Example&endpoint=https://example.com&apiKey=test";

    #[test]
    fn rejects_oversized_urls_before_parsing_and_base64_before_decoding() {
        assert!(
            parse_deeplink_url(&format!("{LINK}&notes={}", "x".repeat(MAX_DEEPLINK_BYTES)))
                .is_err()
        );
        assert!(super::super::utils::decode_base64_param(
            "config",
            &"A".repeat(MAX_DEEPLINK_BYTES + 1)
        )
        .is_err());
        assert!(parse_deeplink_url(&format!(
            "{LINK}&notes={}",
            "x".repeat(MAX_DEEPLINK_BYTES - LINK.len() - 7)
        ))
        .is_ok());
    }

    #[test]
    fn rejects_ambiguous_parameters_and_invalid_typed_values() {
        for suffix in [
            "&app=codex",
            "&%61pp=codex",
            "&enabled=0",
            "&enabled=False",
            "&usageEnabled=no",
            "&usageAutoInterval=-1",
            "&usageAutoInterval=1.5",
            "&usageAutoInterval=18446744073709551616",
        ] {
            assert!(
                parse_deeplink_url(&format!("{LINK}{suffix}")).is_err(),
                "{suffix}"
            );
        }
        let parsed = parse_deeplink_url(&format!(
            "{LINK}&enabled=false&usageEnabled=true&usageAutoInterval=0"
        ))
        .unwrap();
        assert_eq!(parsed.enabled, Some(false));
        assert_eq!(parsed.usage_enabled, Some(true));
        assert_eq!(parsed.usage_auto_interval, Some(0));
    }

    #[test]
    fn rejects_unsupported_url_parts_and_excessive_parameters() {
        for link in [
            LINK.replace("v1/", "user:secret@v1/"),
            LINK.replace("v1/", "v1:123/"),
            format!("{LINK}#ignored"),
        ] {
            assert!(parse_deeplink_url(&link).is_err());
        }
        let params: String = (0..65).map(|i| format!("&extra{i}=x")).collect();
        assert!(parse_deeplink_url(&format!("{LINK}{params}")).is_err());
    }

    #[test]
    fn retired_provider_links_report_unsupported_apps() {
        for app in [
            "gemini", "omp", "ohmypi", "oh-my-pi", "oh_my_pi", "copilot", "xai",
        ] {
            let error =
                parse_deeplink_url(&LINK.replace("app=claude", &format!("app={app}"))).unwrap_err();
            assert!(
                error.to_string().contains("Invalid provider app type"),
                "{error}"
            );
        }
    }

    #[test]
    fn skill_enabled_parameter_is_preserved() {
        let request =
            parse_deeplink_url("ccswitch://v1/import?resource=skill&repo=owner/repo&enabled=false")
                .unwrap();
        assert_eq!(request.enabled, Some(false));
    }

    #[test]
    fn validates_config_derived_and_direct_ipc_urls() {
        use base64::Engine;
        for app in ["claude", "opencode", "dsh"] {
            let config = match app {
                "claude" => serde_json::json!({"env": {"ANTHROPIC_BASE_URL": "file:///private"}}),

                _ => serde_json::json!({"baseUrl": "file://private"}),
            };
            let request = DeepLinkImportRequest {
                resource: "provider".into(),
                app: Some(app.into()),
                config: Some(base64::prelude::BASE64_STANDARD.encode(config.to_string())),
                ..Default::default()
            };
            assert!(
                super::super::parse_and_merge_config(&request).is_err(),
                "{app}"
            );
        }
        let request = DeepLinkImportRequest {
            resource: "provider".into(),
            endpoint: Some("javascript:alert(1)".into()),
            ..Default::default()
        };
        assert!(super::super::parse_and_merge_config(&request).is_err());
    }
}

/// Parse provider deep link parameters
/// 内置图标名的形状：小写字母、数字、`-`、`_`、`.`，不超过 64 个字符。
/// 只看形状，不核对图标是否存在（前端找不到时显示首字母）。
fn is_builtin_icon_name(name: &str) -> bool {
    !name.is_empty()
        && name.len() <= 64
        && name
            .chars()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || matches!(c, '-' | '_' | '.'))
}

fn parse_provider_deeplink(
    params: &HashMap<String, String>,
    version: String,
    resource: String,
) -> Result<DeepLinkImportRequest, AppError> {
    let app = params
        .get("app")
        .ok_or_else(|| AppError::InvalidInput("Missing 'app' parameter".to_string()))?
        .clone();

    // Validate app type against the shared supported-client list.
    if app.parse::<crate::AppType>().is_err() {
        return Err(AppError::InvalidInput(format!(
            "Invalid provider app type: '{app}'"
        )));
    }

    let name = params
        .get("name")
        .ok_or_else(|| AppError::InvalidInput("Missing 'name' parameter".to_string()))?
        .clone();

    // Make these optional for config file auto-fill (v3.8+)
    let homepage = params.get("homepage").cloned();
    let endpoint = params.get("endpoint").cloned();
    let api_key = params.get("apiKey").cloned();

    // Validate URLs only if provided
    if let Some(ref hp) = homepage {
        if !hp.is_empty() {
            validate_url(hp, "homepage")?;
        }
    }
    // Validate each endpoint (supports comma-separated multiple URLs)
    if let Some(ref ep) = endpoint {
        for (i, url) in ep.split(',').enumerate() {
            let trimmed = url.trim();
            if !trimmed.is_empty() {
                validate_url(trimmed, &format!("endpoint[{i}]"))?;
            }
        }
    }

    // Extract optional fields
    let model = params.get("model").cloned();
    // Pi 的节点没有模型就选不到任何模型（见 `provider::build_pi_settings`）

    let notes = params.get("notes").cloned();
    let haiku_model = params.get("haikuModel").cloned();
    let sonnet_model = params.get("sonnetModel").cloned();
    let opus_model = params.get("opusModel").cloned();
    // 在解析阶段就核对，用户当场看到「这个应用不支持这种接口」，而不是确认导入之后才失败
    let api_format = params.get("apiFormat").cloned();
    if let Some(ref format) = api_format {
        super::provider::check_api_format(&app, format)?;
    }
    // icon 只收内置图标名。图片要走下载后重新编码的流程，不能借这个参数直接把
    // data URI 之类的内容塞进库；不合格的值丢掉，退回首字母图标，不让整条链接失败
    let icon = params
        .get("icon")
        .map(|v| v.trim().to_lowercase())
        .filter(|v| is_builtin_icon_name(v));
    // iconUrl 只在确认框里由专门的命令下载（见 `icon_fetch`）。形状不合格的地址同样丢掉，
    // 不让整条链接失败
    let icon_url = params
        .get("iconUrl")
        .and_then(|v| super::icon_fetch::parse_icon_url(v))
        .map(|url| url.to_string());
    let config = params.get("config").cloned();
    let config_format = params.get("configFormat").cloned();
    // 远程配置还没实现。在解析阶段就拒绝，用户当场看到原因，而不是确认导入之后才失败
    if params.contains_key("configUrl") {
        return Err(AppError::InvalidInput(
            "'configUrl' is not supported yet; pass the config inline with 'config' (Base64)"
                .to_string(),
        ));
    }
    let config_url = None;
    let enabled = optional_bool(params, "enabled")?;

    // Extract usage script fields (v3.9+)
    let usage_enabled = optional_bool(params, "usageEnabled")?;
    let usage_script = params.get("usageScript").cloned();
    let usage_api_key = params.get("usageApiKey").cloned();
    let usage_base_url = params.get("usageBaseUrl").cloned();
    let usage_access_token = params.get("usageAccessToken").cloned();
    let usage_user_id = params.get("usageUserId").cloned();
    let usage_auto_interval = params
        .get("usageAutoInterval")
        .map(|v| {
            v.parse::<u64>().map_err(|_| {
                AppError::InvalidInput("'usageAutoInterval' must be a non-negative integer".into())
            })
        })
        .transpose()?;

    let request = DeepLinkImportRequest {
        version,
        resource,
        app: Some(app),
        name: Some(name),
        enabled,
        homepage,
        endpoint,
        api_key,
        icon,
        model,
        notes,
        haiku_model,
        sonnet_model,
        opus_model,
        api_format,
        content: None,
        description: None,
        apps: None,
        repo: None,
        directory: None,
        branch: None,
        config,
        config_format,
        config_url,
        icon_url,
        usage_enabled,
        usage_script,
        usage_api_key,
        usage_base_url,
        usage_access_token,
        usage_user_id,
        usage_auto_interval,
    };
    // 累加式应用会把某些写法当成命令、环境变量或文件引用，解析阶段就拒绝
    if let Ok(app_type) = request
        .app
        .as_deref()
        .unwrap_or("")
        .parse::<crate::AppType>()
    {
        super::provider::check_link_values(&app_type, &request)?;
    }
    Ok(request)
}

/// Parse prompt deep link parameters
fn parse_prompt_deeplink(
    params: &HashMap<String, String>,
    version: String,
    resource: String,
) -> Result<DeepLinkImportRequest, AppError> {
    let app = params
        .get("app")
        .ok_or_else(|| AppError::InvalidInput("Missing 'app' parameter for prompt".to_string()))?
        .clone();

    app.parse::<crate::AppType>()?;

    let name = params
        .get("name")
        .ok_or_else(|| AppError::InvalidInput("Missing 'name' parameter for prompt".to_string()))?
        .clone();

    let content = params
        .get("content")
        .ok_or_else(|| {
            AppError::InvalidInput("Missing 'content' parameter for prompt".to_string())
        })?
        .clone();

    let description = params.get("description").cloned();
    let enabled = optional_bool(params, "enabled")?;

    Ok(DeepLinkImportRequest {
        version,
        resource,
        app: Some(app),
        name: Some(name),
        enabled,
        content: Some(content),
        description,
        icon: None,
        homepage: None,
        endpoint: None,
        api_key: None,
        model: None,
        notes: None,
        haiku_model: None,
        sonnet_model: None,
        opus_model: None,
        api_format: None,
        apps: None,
        repo: None,
        directory: None,
        branch: None,
        config: None,
        config_format: None,
        config_url: None,
        icon_url: None,
        usage_enabled: None,
        usage_script: None,
        usage_api_key: None,
        usage_base_url: None,
        usage_access_token: None,
        usage_user_id: None,
        usage_auto_interval: None,
    })
}

/// Parse MCP deep link parameters
fn parse_mcp_deeplink(
    params: &HashMap<String, String>,
    version: String,
    resource: String,
) -> Result<DeepLinkImportRequest, AppError> {
    let apps = params
        .get("apps")
        .ok_or_else(|| AppError::InvalidInput("Missing 'apps' parameter for MCP".to_string()))?
        .clone();

    super::mcp::parse_mcp_apps(&apps)?;

    let config = params
        .get("config")
        .ok_or_else(|| AppError::InvalidInput("Missing 'config' parameter for MCP".to_string()))?
        .clone();

    let enabled = optional_bool(params, "enabled")?;

    Ok(DeepLinkImportRequest {
        version,
        resource,
        apps: Some(apps),
        enabled,
        config: Some(config),
        config_format: Some("json".to_string()), // MCP config is always JSON
        app: None,
        name: None,
        icon: None,
        homepage: None,
        endpoint: None,
        api_key: None,
        model: None,
        notes: None,
        haiku_model: None,
        sonnet_model: None,
        opus_model: None,
        api_format: None,
        content: None,
        description: None,
        repo: None,
        directory: None,
        branch: None,
        config_url: None,
        icon_url: None,
        usage_enabled: None,
        usage_script: None,
        usage_api_key: None,
        usage_base_url: None,
        usage_access_token: None,
        usage_user_id: None,
        usage_auto_interval: None,
    })
}

/// Parse skill deep link parameters
fn parse_skill_deeplink(
    params: &HashMap<String, String>,
    version: String,
    resource: String,
) -> Result<DeepLinkImportRequest, AppError> {
    let repo = params
        .get("repo")
        .ok_or_else(|| AppError::InvalidInput("Missing 'repo' parameter for skill".to_string()))?
        .clone();

    // Validate repo format (should be "owner/name")
    if !repo.contains('/') || repo.split('/').count() != 2 {
        return Err(AppError::InvalidInput(format!(
            "Invalid repo format: expected 'owner/name', got '{repo}'"
        )));
    }

    let directory = params.get("directory").cloned();
    let branch = params.get("branch").cloned();

    Ok(DeepLinkImportRequest {
        version,
        resource,
        repo: Some(repo),
        directory,
        branch,
        icon: None,
        app: None, // Skill links add a shared repository; app installation is a separate action.
        name: None,
        enabled: optional_bool(params, "enabled")?,
        homepage: None,
        endpoint: None,
        api_key: None,
        model: None,
        notes: None,
        haiku_model: None,
        sonnet_model: None,
        opus_model: None,
        api_format: None,
        content: None,
        description: None,
        apps: None,
        config: None,
        config_format: None,
        config_url: None,
        icon_url: None,
        usage_enabled: None,
        usage_script: None,
        usage_api_key: None,
        usage_base_url: None,
        usage_access_token: None,
        usage_user_id: None,
        usage_auto_interval: None,
    })
}
