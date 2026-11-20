use super::*;

pub(super) struct CopilotRequestContext {
    pub(super) classification: super::super::copilot_optimizer::CopilotClassification,
    pub(super) request_id: Option<String>,
    pub(super) interaction_id: Option<String>,
}

impl CopilotRequestContext {
    pub(super) fn apply_headers(
        &self,
        headers: &mut Vec<(http::HeaderName, http::HeaderValue)>,
        request_classification: bool,
    ) -> Result<(), ProxyError> {
        let request_id = self
            .request_id
            .as_deref()
            .map(http::HeaderValue::from_str)
            .transpose()
            .map_err(|_| ProxyError::ConfigError("Invalid Copilot request ID".to_string()))?;
        let interaction_id = self
            .interaction_id
            .as_deref()
            .map(http::HeaderValue::from_str)
            .transpose()
            .map_err(|_| ProxyError::ConfigError("Invalid Copilot interaction ID".to_string()))?;

        for (name, value) in headers.iter_mut() {
            match name.as_str() {
                "x-initiator" if request_classification => {
                    *value = http::HeaderValue::from_static(self.classification.initiator);
                }
                "x-interaction-type" if self.classification.is_subagent => {
                    *value = http::HeaderValue::from_static("conversation-subagent");
                }
                "x-request-id" | "x-agent-task-id" => {
                    if let Some(request_id) = &request_id {
                        *value = request_id.clone();
                    }
                }
                _ => {}
            }
        }

        if let Some(interaction_id) = interaction_id {
            headers.push((
                http::HeaderName::from_static("x-interaction-id"),
                interaction_id,
            ));
        }
        if self.classification.is_subagent {
            log::info!(
                "[Copilot] 子代理请求: x-initiator=agent, x-interaction-type=conversation-subagent"
            );
        }
        Ok(())
    }
}


pub(super) fn is_managed_copilot_request(app_type: &AppType, provider: &Provider, base_url: &str) -> bool {
    provider
        .meta
        .as_ref()
        .and_then(|meta| meta.provider_type.as_deref())
        == Some("github_copilot")
        || (!matches!(app_type, AppType::Codex) && base_url.contains("githubcopilot.com"))
}

pub(super) fn codex_copilot_lookup_error(model_id: &str, error: CopilotAuthError) -> ProxyError {
    let message =
        format!("Failed to resolve GitHub Copilot capabilities for model {model_id}: {error}");
    match error {
        CopilotAuthError::DeviceFlowNotStarted
        | CopilotAuthError::AuthorizationPending
        | CopilotAuthError::AccessDenied
        | CopilotAuthError::ExpiredToken
        | CopilotAuthError::GitHubTokenInvalid
        | CopilotAuthError::NoCopilotSubscription
        | CopilotAuthError::AccountNotFound(_) => ProxyError::AuthError(message),
        CopilotAuthError::NetworkError(_) | CopilotAuthError::CopilotTokenFetchFailed(_) => {
            ProxyError::ForwardFailed(message)
        }
        CopilotAuthError::ParseError(_)
        | CopilotAuthError::IoError(_)
        | CopilotAuthError::InvalidDomain(_) => ProxyError::ConfigError(message),
    }
}

pub(super) fn apply_codex_copilot_model(
    body: &mut Value,
    resolved: Option<ResolvedCopilotModel>,
    api_format: CodexCopilotApiFormat,
) -> Result<CopilotTransport, ProxyError> {
    let requested = match api_format {
        CodexCopilotApiFormat::Auto => "a Responses or Chat Completions endpoint",
        CodexCopilotApiFormat::OpenaiResponses => {
            "the requested Responses endpoint (codexCopilotApiFormat=openai_responses)"
        }
        CodexCopilotApiFormat::OpenaiChat => {
            "the requested Chat Completions endpoint (codexCopilotApiFormat=openai_chat)"
        }
    };
    let Some(resolved) = resolved else {
        let model = body
            .get("model")
            .and_then(Value::as_str)
            .unwrap_or("<missing>");
        return Err(ProxyError::ConfigError(format!(
            "GitHub Copilot model {model} is unavailable for {requested} in this provider's catalogue"
        )));
    };
    let Some(transport) = resolved.transport else {
        return Err(ProxyError::ConfigError(format!(
            "GitHub Copilot model {} does not advertise {requested}",
            resolved.id
        )));
    };
    body["model"] = Value::String(resolved.id);
    Ok(transport)
}

/// Pick the Copilot upstream protocol for a Claude-client request.
///
/// Anthropic models that advertise `/v1/messages` are passed through natively:
/// the Chat Completions bridge drops `cache_control` breakpoints and Copilot's
/// chat endpoint stops prefix caching at the first mid-conversation `system`
/// message Claude Code inserts, so every turn re-bills the whole history.
pub(super) fn copilot_api_format_for_model(
    vendor: Option<&str>,
    supported_endpoints: &[String],
) -> &'static str {
    if vendor.is_some_and(|vendor| vendor.eq_ignore_ascii_case("openai")) {
        "openai_responses"
    } else if vendor.is_some_and(|vendor| vendor.eq_ignore_ascii_case("anthropic"))
        && supported_endpoints
            .iter()
            .any(|endpoint| endpoint.trim_end_matches('/') == "/v1/messages")
    {
        "anthropic"
    } else if transport_for(supported_endpoints, CodexCopilotApiFormat::OpenaiResponses).is_some()
        && transport_for(supported_endpoints, CodexCopilotApiFormat::OpenaiChat).is_none()
    {
        // Copilot rejects these models on the chat endpoint (e.g. xAI grok)
        "openai_responses"
    } else {
        "openai_chat"
    }
}

