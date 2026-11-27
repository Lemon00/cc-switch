//! Shared reply handling for the balance and coding plan queries.

use reqwest::{RequestBuilder, StatusCode};

const MAX_USAGE_RESPONSE_BYTES: usize = 4 * 1024 * 1024;
const MAX_ERROR_BODY_CHARS: usize = 1024;

pub fn parse_provider_url(base_url: &str) -> Option<url::Url> {
    let base_url = base_url.trim();
    let parsed = if base_url.contains("://") {
        url::Url::parse(base_url)
    } else {
        url::Url::parse(&format!("https://{base_url}"))
    }
    .ok()?;
    (matches!(parsed.scheme(), "http" | "https") && parsed.has_host()).then_some(parsed)
}

/// A definite answer from a usage endpoint.
pub enum JsonReply {
    /// A 2xx response with a JSON body.
    Body(serde_json::Value),
    /// HTTP 401 or 403.
    Denied(StatusCode),
    /// Any other status, or a complete body that is not JSON. Error excerpts are bounded.
    Failed(String),
}

/// Sends `request` and reads its JSON reply.
///
/// `Err` means the request didn't complete (send failed or the body was cut off), which is
/// transient and gets retried. Everything the server decided comes back as a [`JsonReply`].
pub async fn send_for_json(request: RequestBuilder) -> Result<JsonReply, String> {
    let resp = request
        .send()
        .await
        .map_err(|error| format!("Network error: {}", error.without_url()))?;

    parse_reply(resp).await
}

async fn parse_reply(resp: reqwest::Response) -> Result<JsonReply, String> {
    let status = resp.status();
    if status == StatusCode::UNAUTHORIZED || status == StatusCode::FORBIDDEN {
        return Ok(JsonReply::Denied(status));
    }
    if !status.is_success() {
        let raw =
            crate::proxy::http_client::read_body_with_limit(resp, MAX_USAGE_RESPONSE_BYTES).await?;
        let body = String::from_utf8_lossy(&raw);
        let excerpt: String = body.chars().take(MAX_ERROR_BODY_CHARS).collect();
        let suffix = if body.chars().count() > MAX_ERROR_BODY_CHARS {
            "…"
        } else {
            ""
        };
        return Ok(JsonReply::Failed(format!(
            "API error (HTTP {status}): {excerpt}{suffix}"
        )));
    }

    // 先 bytes() 再解析：读体失败（超时/连接中断）是瞬时 → Err；拿到完整响应体
    // 后解析失败才是确定性。reqwest 的 json() 把读体错误也包成 decode，无法区分。
    let raw = crate::proxy::http_client::read_body_with_limit(resp, MAX_USAGE_RESPONSE_BYTES)
        .await
        .map_err(|error| format!("Failed to read response: {error}"))?;
    Ok(match serde_json::from_slice(&raw) {
        Ok(body) => JsonReply::Body(body),
        Err(e) => JsonReply::Failed(format!("Failed to parse response: {e}")),
    })
}

/// 解析 JSON 值为 f64，兼容数字和字符串格式（如 `100` 和 `"100"`）
pub fn parse_f64(value: &serde_json::Value) -> Option<f64> {
    value
        .as_f64()
        .or_else(|| value.as_str().and_then(|s| s.parse().ok()))
        .filter(|value| value.is_finite())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn response(status: u16, body: String) -> reqwest::Response {
        http::Response::builder()
            .status(status)
            .body(body)
            .unwrap()
            .into()
    }

    #[tokio::test]
    async fn rejects_oversized_success_and_error_replies() {
        for status in [200, 500] {
            let reply = response(status, "x".repeat(MAX_USAGE_RESPONSE_BYTES + 1));
            assert!(parse_reply(reply).await.is_err());
        }
    }

    #[tokio::test]
    async fn bounds_unicode_error_excerpts() {
        let reply = response(429, "错".repeat(MAX_ERROR_BODY_CHARS + 10));
        let JsonReply::Failed(message) = parse_reply(reply).await.unwrap() else {
            panic!("expected API failure");
        };
        assert_eq!(message.matches('错').count(), MAX_ERROR_BODY_CHARS);
        assert!(message.ends_with('…'));
    }

    #[tokio::test]
    async fn preserves_auth_success_and_parse_error_categories() {
        assert!(matches!(
            parse_reply(response(401, String::new())).await.unwrap(),
            JsonReply::Denied(StatusCode::UNAUTHORIZED)
        ));
        assert!(matches!(
            parse_reply(response(200, "{}".into())).await.unwrap(),
            JsonReply::Body(_)
        ));
        assert!(matches!(
            parse_reply(response(200, "not-json".into())).await.unwrap(),
            JsonReply::Failed(_)
        ));
    }

    #[test]
    fn rejects_non_finite_numeric_strings() {
        for value in ["NaN", "inf", "-inf", "1e999"] {
            assert_eq!(parse_f64(&serde_json::json!(value)), None);
        }
        assert_eq!(parse_f64(&serde_json::json!("12.5")), Some(12.5));
        assert_eq!(parse_f64(&serde_json::json!(0)), Some(0.0));
    }
}
