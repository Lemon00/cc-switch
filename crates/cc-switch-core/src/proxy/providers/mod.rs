//! Provider Adapters Module
//!
//! 供应商适配器模块，提供统一的接口抽象不同上游供应商的处理逻辑。
//!
//! ## 模块结构
//! - `adapter`: 定义 `ProviderAdapter` trait
//! - `auth`: 认证类型和策略
//! - `claude`: Claude (Anthropic) 适配器
//! - `codex`: Codex (OpenAI) 适配器
//! - `gemini`: Gemini (Google) 适配器
//! - `models`: API 数据模型
//! - `transform`: 格式转换

pub mod adapter;
pub mod auth;
pub mod claude;
pub mod claude_reasoning;
pub mod codex;
pub mod codex_chat_common;
pub mod codex_chat_history;
pub mod codex_chat_tool_history;
pub mod codex_compaction;
pub mod codex_oauth_auth;
pub mod codex_responses_arguments;
pub mod codex_responses_sse;

pub mod gemini;
pub mod gemini_schema;
pub mod gemini_shadow;
pub mod inline_think;
pub mod models;
pub mod reasoning_bridge;
pub mod responses_late_arguments;
pub mod streaming;
pub mod streaming_codex_anthropic;
pub mod streaming_codex_chat;
pub mod streaming_gemini;
pub mod streaming_responses;
pub mod transform;
pub mod transform_codex_anthropic;
pub mod transform_codex_chat;
pub mod transform_codex_chat_gemini_schema;
pub mod transform_codex_chat_moonshot_schema;
pub mod transform_codex_gemini;
pub mod transform_codex_responses_kimi_history;

pub(crate) mod codex_responses_receipts;
pub mod transform_codex_responses_reorder;

pub mod transform_gemini;
pub mod transform_responses;

use crate::app_config::AppType;
use crate::provider::Provider;
use serde::{Deserialize, Serialize};

pub const CHATGPT_CODEX_BASE_URL: &str = "https://chatgpt.com/backend-api/codex";

/// 上游是不是 Mistral 官方 API。只认 `api.mistral.ai` 这个主机：名字、路径或
/// 模型名里带 "mistral" 的中转站不算，它们照各自的协议处理。
pub fn is_mistral_api(base_url: &str) -> bool {
    url::Url::parse(base_url).is_ok_and(|url| url.host_str() == Some("api.mistral.ai"))
}

/// 上游主机是 `suffixes` 里的某一个，或是它的子域名（按标签边界比，不按子串）。
/// 解析不了或没有主机时为 false。
fn url_host_is_or_under(base_url: &str, suffixes: &[&str]) -> bool {
    let Ok(url) = url::Url::parse(base_url.trim()) else {
        return false;
    };
    let Some(host) = url.host_str() else {
        return false;
    };
    let host = host.to_ascii_lowercase();
    suffixes.iter().any(|suffix| {
        host == *suffix
            || host
                .strip_suffix(suffix)
                .is_some_and(|prefix| prefix.ends_with('.'))
    })
}

// 公开导出
pub use adapter::ProviderAdapter;
pub use auth::{AuthInfo, AuthStrategy};
pub use claude::{
    claude_api_format_needs_transform, get_claude_api_format,
    normalize_anthropic_messages_for_provider, transform_claude_request_for_api_format,
    ClaudeAdapter,
};
pub use codex::CodexAdapter;
pub use codex::{
    apply_codex_chat_upstream_model, apply_codex_upstream_model, codex_provider_upstream_model,
    codex_stack_upstream_rejects_web_search, inject_codex_chat_prompt_cache_key,
    is_codex_official_provider, is_codex_responses_endpoint,
    provider_needs_responses_late_arguments_repair, resolve_codex_catalog_tool_profile,
    resolve_codex_chat_reasoning_config, should_convert_codex_responses_to_anthropic,
    should_convert_codex_responses_to_chat, strip_codex_hosted_web_search,
    strip_codex_openai_only_request_fields,
};
pub use gemini::GeminiAdapter;

/// 供应商类型枚举
///
/// 区分不同供应商的具体实现方式，决定认证和请求处理逻辑。
/// 比 AppType 更细粒度，支持同一 AppType 下的多种变体。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProviderType {
    /// Anthropic 官方 API (x-api-key + anthropic-version)
    Claude,
    /// Claude 中转服务 (仅 Bearer 认证，无 x-api-key)
    ClaudeAuth,
    /// OpenAI Codex Response API
    Codex,
    /// Google Gemini API (x-goog-api-key)
    Gemini,
    /// Google Gemini 上游协议 (OAuth Bearer)
    #[serde(rename = "gemini_oauth")]
    GeminiOAuth,
    /// OpenRouter（已支持 Claude Code 兼容接口，默认透传；保留旧转换逻辑备用）
    OpenRouter,

    /// OpenAI Codex (ChatGPT Plus/Pro OAuth，需要 Anthropic ↔ Responses API 转换)
    CodexOAuth,
}

impl ProviderType {
    #[allow(dead_code)]
    pub fn needs_transform(&self) -> bool {
        match self {
            ProviderType::CodexOAuth => true,

            ProviderType::OpenRouter => false,
            _ => false,
        }
    }

    /// 获取默认端点
    #[allow(dead_code)]
    pub fn default_endpoint(&self) -> &'static str {
        match self {
            ProviderType::Claude | ProviderType::ClaudeAuth => "https://api.anthropic.com",
            ProviderType::Codex => "https://api.openai.com",
            ProviderType::Gemini | ProviderType::GeminiOAuth => {
                "https://generativelanguage.googleapis.com"
            }
            ProviderType::OpenRouter => "https://openrouter.ai/api",

            ProviderType::CodexOAuth => CHATGPT_CODEX_BASE_URL,
        }
    }

    /// 从 AppType 和 Provider 配置推断供应商类型
    ///
    /// 根据配置中的 base_url、auth_mode、api_key 格式等信息推断具体的供应商类型
    #[allow(dead_code)]
    pub fn from_app_type_and_config(app_type: &AppType, provider: &Provider) -> Option<Self> {
        let provider_type = match app_type {
            AppType::Claude => {
                if get_claude_api_format(provider) == "gemini_native" {
                    let adapter = ClaudeAdapter::new();
                    return Some(
                        match adapter.extract_auth(provider).map(|auth| auth.strategy) {
                            Some(AuthStrategy::GoogleOAuth) => ProviderType::GeminiOAuth,
                            _ => ProviderType::Gemini,
                        },
                    );
                }

                if let Some(meta) = provider.meta.as_ref() {
                    if meta.provider_type.as_deref() == Some("codex_oauth") {
                        return Some(ProviderType::CodexOAuth);
                    }
                }

                let adapter = ClaudeAdapter::new();
                if let Ok(base_url) = adapter.extract_base_url(provider) {
                    // 检测是否为 OpenRouter
                    if base_url.contains("openrouter.ai") {
                        return Some(ProviderType::OpenRouter);
                    }
                }
                // 检测是否为中转服务（仅 Bearer 认证）
                // 注意：ProviderMeta 没有直接的 auth_mode 字段，
                // 我们通过检查 settings_config 中的配置来判断
                // 检查 settings_config 中的 auth_mode
                if let Some(auth_mode) = provider
                    .settings_config
                    .get("auth_mode")
                    .and_then(|v| v.as_str())
                {
                    if auth_mode == "bearer_only" {
                        return Some(ProviderType::ClaudeAuth);
                    }
                }
                // 检查 env 中的 auth_mode
                if let Some(env) = provider.settings_config.get("env") {
                    if let Some(auth_mode) = env.get("AUTH_MODE").and_then(|v| v.as_str()) {
                        if auth_mode == "bearer_only" {
                            return Some(ProviderType::ClaudeAuth);
                        }
                    }
                }
                ProviderType::Claude
            }
            AppType::Codex => ProviderType::Codex,

            AppType::OpenCode => ProviderType::Codex,
            AppType::Dsh | AppType::Agy => return None,
        };
        Some(provider_type)
    }

    /// 转换为字符串表示
    pub fn as_str(&self) -> &'static str {
        match self {
            ProviderType::Claude => "claude",
            ProviderType::ClaudeAuth => "claude_auth",
            ProviderType::Codex => "codex",
            ProviderType::Gemini => "gemini",
            ProviderType::GeminiOAuth => "gemini_oauth",
            ProviderType::OpenRouter => "openrouter",

            ProviderType::CodexOAuth => "codex_oauth",
        }
    }
}

impl std::fmt::Display for ProviderType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for ProviderType {
    type Err = String;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "claude" => Ok(ProviderType::Claude),
            "claude_auth" | "claude-auth" => Ok(ProviderType::ClaudeAuth),
            "codex" => Ok(ProviderType::Codex),
            "gemini" => Ok(ProviderType::Gemini),
            "gemini_oauth" | "gemini-oauth" => Ok(ProviderType::GeminiOAuth),
            "openrouter" => Ok(ProviderType::OpenRouter),

            "codex_oauth" | "codex-oauth" | "codexoauth" => Ok(ProviderType::CodexOAuth),

            _ => Err(format!("Invalid provider type: {s}")),
        }
    }
}

/// 根据 AppType 获取对应的适配器
pub fn get_adapter(app_type: &AppType) -> Option<Box<dyn ProviderAdapter>> {
    Some(match app_type {
        AppType::Claude => Box::new(ClaudeAdapter::new()),
        AppType::Codex => Box::new(CodexAdapter::new()),

        AppType::OpenCode => Box::new(CodexAdapter::new()),
        AppType::Dsh | AppType::Agy => return None,
    })
}

/// 根据 ProviderType 获取对应的适配器
#[allow(dead_code)]
pub fn get_adapter_for_provider_type(provider_type: &ProviderType) -> Box<dyn ProviderAdapter> {
    match provider_type {
        ProviderType::Claude
        | ProviderType::ClaudeAuth
        | ProviderType::OpenRouter
        | ProviderType::CodexOAuth => Box::new(ClaudeAdapter::new()),
        ProviderType::Codex => Box::new(CodexAdapter::new()),
        ProviderType::Gemini | ProviderType::GeminiOAuth => Box::new(GeminiAdapter::new()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn create_provider(config: serde_json::Value) -> Provider {
        Provider {
            id: "test".to_string(),
            name: "Test Provider".to_string(),
            settings_config: config,
            website_url: None,
            category: None,
            created_at: None,
            sort_index: None,
            notes: None,
            meta: None,
            icon: None,
            icon_color: None,
            in_failover_queue: false,
        }
    }

    #[test]
    fn test_provider_type_needs_transform() {
        assert!(!ProviderType::Claude.needs_transform());
        assert!(!ProviderType::ClaudeAuth.needs_transform());
        assert!(!ProviderType::Codex.needs_transform());
        assert!(!ProviderType::Gemini.needs_transform());
        assert!(!ProviderType::GeminiOAuth.needs_transform());
        assert!(!ProviderType::OpenRouter.needs_transform());
    }

    #[test]
    fn test_provider_type_default_endpoint() {
        assert_eq!(
            ProviderType::Claude.default_endpoint(),
            "https://api.anthropic.com"
        );
        assert_eq!(
            ProviderType::ClaudeAuth.default_endpoint(),
            "https://api.anthropic.com"
        );
        assert_eq!(
            ProviderType::Codex.default_endpoint(),
            "https://api.openai.com"
        );
        assert_eq!(
            ProviderType::Gemini.default_endpoint(),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            ProviderType::GeminiOAuth.default_endpoint(),
            "https://generativelanguage.googleapis.com"
        );
        assert_eq!(
            ProviderType::OpenRouter.default_endpoint(),
            "https://openrouter.ai/api"
        );
    }

    #[test]
    fn test_provider_type_from_str() {
        assert_eq!(
            "claude".parse::<ProviderType>().unwrap(),
            ProviderType::Claude
        );
        assert_eq!(
            "claude_auth".parse::<ProviderType>().unwrap(),
            ProviderType::ClaudeAuth
        );
        assert_eq!(
            "claude-auth".parse::<ProviderType>().unwrap(),
            ProviderType::ClaudeAuth
        );
        assert_eq!(
            "codex".parse::<ProviderType>().unwrap(),
            ProviderType::Codex
        );
        assert_eq!(
            "gemini".parse::<ProviderType>().unwrap(),
            ProviderType::Gemini
        );
        assert_eq!(
            "gemini_oauth".parse::<ProviderType>().unwrap(),
            ProviderType::GeminiOAuth
        );
        assert_eq!(
            "gemini-oauth".parse::<ProviderType>().unwrap(),
            ProviderType::GeminiOAuth
        );
        assert_eq!(
            "openrouter".parse::<ProviderType>().unwrap(),
            ProviderType::OpenRouter
        );

        assert!("invalid".parse::<ProviderType>().is_err());
    }

    #[test]
    fn test_provider_type_as_str() {
        assert_eq!(ProviderType::Claude.as_str(), "claude");
        assert_eq!(ProviderType::ClaudeAuth.as_str(), "claude_auth");
        assert_eq!(ProviderType::Codex.as_str(), "codex");
        assert_eq!(ProviderType::Gemini.as_str(), "gemini");
        assert_eq!(ProviderType::GeminiOAuth.as_str(), "gemini_oauth");
        assert_eq!(ProviderType::OpenRouter.as_str(), "openrouter");
    }

    #[test]
    fn test_provider_type_serde() {
        // Test serialization
        let claude = ProviderType::Claude;
        let serialized = serde_json::to_string(&claude).unwrap();
        assert_eq!(serialized, "\"claude\"");

        let claude_auth = ProviderType::ClaudeAuth;
        let serialized = serde_json::to_string(&claude_auth).unwrap();
        assert_eq!(serialized, "\"claude_auth\"");

        // Test deserialization
        let deserialized: ProviderType = serde_json::from_str("\"claude\"").unwrap();
        assert_eq!(deserialized, ProviderType::Claude);

        let deserialized: ProviderType = serde_json::from_str("\"gemini_oauth\"").unwrap();
        assert_eq!(deserialized, ProviderType::GeminiOAuth);
    }

    #[test]
    fn test_from_app_type_claude_direct() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://api.anthropic.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-ant-test"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, Some(ProviderType::Claude));
    }

    #[test]
    fn test_from_app_type_claude_openrouter() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://openrouter.ai/api",
                "OPENROUTER_API_KEY": "sk-or-test"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, Some(ProviderType::OpenRouter));
    }

    #[test]
    fn test_from_app_type_claude_auth() {
        let provider = create_provider(json!({
            "env": {
                "ANTHROPIC_BASE_URL": "https://some-proxy.com",
                "ANTHROPIC_AUTH_TOKEN": "sk-test"
            },
            "auth_mode": "bearer_only"
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Claude, &provider);
        assert_eq!(provider_type, Some(ProviderType::ClaudeAuth));
    }

    #[test]
    fn test_from_app_type_codex() {
        let provider = create_provider(json!({
            "env": {
                "OPENAI_API_KEY": "sk-test"
            }
        }));

        let provider_type = ProviderType::from_app_type_and_config(&AppType::Codex, &provider);
        assert_eq!(provider_type, Some(ProviderType::Codex));
    }

    #[test]
    fn mistral_api_is_only_the_official_host() {
        // 官方域名（含带路径 / 大小写差异 / 首尾空白的写法）。
        assert!(is_mistral_api("https://api.mistral.ai/v1"));
        assert!(is_mistral_api("https://api.mistral.ai"));
        assert!(is_mistral_api("https://API.MISTRAL.AI/v1/chat/completions"));
        assert!(is_mistral_api(" https://api.mistral.ai/v1\n"));

        // 其他网关、相似域名、路径里带官方域名的中转、非法 URL 都不算。
        assert!(!is_mistral_api("https://api.deepseek.com"));
        assert!(!is_mistral_api("https://api.mistral.ai.example/v1"));
        assert!(!is_mistral_api("https://relay.example/api.mistral.ai"));
        assert!(!is_mistral_api("https://mistral-relay.example/v1"));
        assert!(!is_mistral_api("not a url"));
        assert!(!is_mistral_api(""));
    }

    #[test]
    fn test_get_adapter_for_provider_type() {
        let adapter = get_adapter_for_provider_type(&ProviderType::Claude);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderType::ClaudeAuth);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderType::OpenRouter);
        assert_eq!(adapter.name(), "Claude");

        let adapter = get_adapter_for_provider_type(&ProviderType::Codex);
        assert_eq!(adapter.name(), "Codex");

        let adapter = get_adapter_for_provider_type(&ProviderType::Gemini);
        assert_eq!(adapter.name(), "Gemini");

        let adapter = get_adapter_for_provider_type(&ProviderType::GeminiOAuth);
        assert_eq!(adapter.name(), "Gemini");
    }
}
