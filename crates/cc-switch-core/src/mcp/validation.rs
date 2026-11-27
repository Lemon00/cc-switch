//! MCP 服务器配置验证模块

use serde_json::Value;

use crate::error::AppError;

/// Normalize only transport fields when comparing imported MCP definitions.
pub fn transport_spec(spec: &Value) -> Value {
    let mut spec = spec.clone();
    if spec.is_object() && spec_transport_type(&spec) == "http" {
        spec["type"] = serde_json::json!("http");
    } else if spec.is_object() && spec.get("type").is_none() && spec.get("command").is_some() {
        spec["type"] = serde_json::json!("stdio");
    }
    if let Some(object) = spec.as_object_mut() {
        object.retain(|key, _| {
            ["command", "args", "env", "cwd", "url", "headers", "type"].contains(&key.as_str())
        });
    }
    spec
}

/// Validate the shared connection schema before writing any native configuration.
pub fn validate_server_spec(spec: &Value) -> Result<(), AppError> {
    if !spec.is_object() {
        return Err(AppError::McpValidation(
            "MCP 服务器连接定义必须为 JSON 对象".into(),
        ));
    }
    for field in ["type", "command", "cwd", "url"] {
        if let Some(value) = spec.get(field) {
            if !value.as_str().is_some_and(|text| !text.contains('\0')) {
                return Err(AppError::McpValidation(format!(
                    "MCP {field} 必须是无 NUL 字符的字符串"
                )));
            }
        }
    }
    if let Some(args) = spec.get("args") {
        if !args.as_array().is_some_and(|args| {
            args.iter()
                .all(|arg| arg.as_str().is_some_and(|arg| !arg.contains('\0')))
        }) {
            return Err(AppError::McpValidation("MCP args 必须是字符串数组".into()));
        }
    }
    for field in ["env", "headers"] {
        if let Some(value) = spec.get(field) {
            let Some(map) = value.as_object() else {
                return Err(AppError::McpValidation(format!(
                    "MCP {field} 必须是字符串映射"
                )));
            };
            for (key, value) in map {
                let valid = value.as_str().is_some_and(|value| {
                    if field == "env" {
                        !key.is_empty() && !key.contains(['=', '\0']) && !value.contains('\0')
                    } else {
                        reqwest::header::HeaderName::from_bytes(key.as_bytes()).is_ok()
                            && reqwest::header::HeaderValue::from_str(value).is_ok()
                    }
                });
                if !valid {
                    return Err(AppError::McpValidation(format!(
                        "MCP {field} 包含无效键或值"
                    )));
                }
            }
        }
    }
    // 与写出侧 `spec_transport_type` 对齐：缺 type 但有 url 视为 http，
    // 不能一律当 stdio（否则 `{"url":…}` 会被这里拒、却被 Claude/Codex 写出）。
    let transport = spec_transport_type(spec);
    let is_stdio = transport == "stdio";
    let is_http = transport == "http";
    let is_sse = transport == "sse";

    if !(is_stdio || is_http || is_sse) {
        return Err(AppError::McpValidation(
            "MCP 服务器 type 必须是 'stdio'、'http' 或 'sse'（或省略表示 stdio）".into(),
        ));
    }

    if is_stdio {
        let cmd = spec.get("command").and_then(|x| x.as_str()).unwrap_or("");
        if cmd.trim().is_empty() {
            return Err(AppError::McpValidation(
                "stdio 类型的 MCP 服务器缺少 command 字段".into(),
            ));
        }
    }
    if is_http || is_sse {
        let url = spec.get("url").and_then(|x| x.as_str()).unwrap_or("");
        if url.trim().is_empty() {
            return Err(AppError::McpValidation(format!(
                "{transport} 类型的 MCP 服务器缺少 url 字段"
            )));
        }
        if !url::Url::parse(url)
            .ok()
            .is_some_and(|url| matches!(url.scheme(), "http" | "https") && url.host_str().is_some())
        {
            return Err(AppError::McpValidation(
                "MCP url 必须是有效的 HTTP 或 HTTPS 地址".into(),
            ));
        }
    }
    Ok(())
}

/// 写出侧的传输类型（各应用把统一规范投影成原生格式时共用）。
///
/// 规范里可能没有 `type`（面板 JSON 模式粘贴 `{"url": …}`、deeplink 都不补），
/// 此时不能一律当 stdio：那样 url 型服务器会被投影成没有 `url` 的 stdio 条目
/// （Codex 报 `url is not supported for stdio` 整份 config.toml 加载失败，
/// Hermes 只剩 `enabled: true` 的空壳）。显式写了 stdio 却只有 `url` 的规范同理。
pub fn spec_transport_type(spec: &Value) -> &str {
    let has = |key: &str| {
        spec.get(key)
            .and_then(|v| v.as_str())
            .is_some_and(|s| !s.trim().is_empty())
    };
    // Pi/AGY 等端使用 streamable-http；统一成 http 再校验与投影。
    let explicit = match spec.get("type").and_then(|v| v.as_str()) {
        Some("streamable-http" | "streamableHttp") => Some("http"),
        other => other,
    };
    if matches!(explicit, None | Some("stdio")) && !has("command") && has("url") {
        return "http";
    }
    explicit.unwrap_or("stdio")
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn rejects_malformed_fields_without_echoing_secrets() {
        for patch in [
            json!({"type": 1}),
            json!({"args": "secret-value"}),
            json!({"args": [1]}),
            json!({"env": {"TOKEN": 123}}),
            json!({"env": {"BAD=KEY": "secret-value"}}),
            json!({"headers": {"Authorization": "secret-value\nInjected: yes"}}),
            json!({"cwd": []}),
            json!({"command": "bad\u{0000}command"}),
        ] {
            let mut spec = json!({"command": "echo"});
            spec.as_object_mut()
                .unwrap()
                .extend(patch.as_object().unwrap().clone());
            let error = validate_server_spec(&spec).unwrap_err().to_string();
            assert!(!error.contains("secret-value"));
        }
        for url in ["file:///secret-value", "https://", "not-a-url"] {
            assert!(validate_server_spec(&json!({"url": url})).is_err());
        }
    }

    #[test]
    fn accepts_empty_arguments_and_supported_http_aliases() {
        validate_server_spec(
            &json!({"command": "echo", "args": ["", "--flag"], "env": {"EMPTY": ""}}),
        )
        .unwrap();
        for kind in ["http", "streamable-http", "streamableHttp", "stdio"] {
            let spec = json!({"type": kind, "command": " ", "url": "https://example.com/mcp"});
            validate_server_spec(&spec).unwrap();
            assert_eq!(spec_transport_type(&spec), "http");
        }
    }
}
