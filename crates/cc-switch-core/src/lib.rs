//! cc-switch backend core: database, providers, proxy, live writers.
//! No Tauri, no plugins, no WebKit. GUI / CLI / TUI are frontends on top.

/// 前端（GUI / CLI / TUI）沿用官方库根的常用名字，不用改调用点。
pub use crate::app_config::AppType;
pub use crate::database::Database;
pub use crate::proxy::usage::logger::set_cli_usage_recording;
pub use crate::services::ProviderService;
pub use crate::store::AppState;

pub mod agy_config;
pub mod app_config;
pub mod background;
pub mod codex_config;
pub mod config;
pub mod database;
pub mod error;
pub mod events;
pub mod live;
pub mod kimi_config;
#[cfg(feature = "session-monitor")]
pub mod kimi_session;
pub mod claude_desktop_config;
pub mod pi_config;
pub mod grok_config;
pub mod dsh_config;
pub mod mcode_config;
pub mod hermes_config;
pub mod gemini_config;
pub mod openclaw_config;
pub mod opencode_config;
pub mod omp_config;
pub mod codex_state_db;
pub mod usage_script;
pub mod usage_events;
pub mod mcp;
pub mod codex_rollout_file;
pub mod jsonc_document;
pub mod claude_mcp;
pub mod gemini_mcp;
pub mod mode;
pub mod model_capabilities;
pub mod prompt;
pub mod prompt_files;
pub mod provider;
pub mod process_util;
pub mod provider_routing;
pub mod proxy;
pub mod services;
pub mod settings;
#[cfg(feature = "session-monitor")]
pub mod session_manager;
pub mod store;

/// Run an async future from a synchronous context, without Tauri.
///
/// A fresh OS thread with its own current-thread runtime is used on purpose:
/// this is only called from low-frequency sync paths (provider switch etc.),
/// and it is safe both inside and outside a running tokio runtime, unlike
/// blocking a worker thread.
pub fn block_on<F: std::future::Future + Send>(f: F) -> F::Output
where
    F::Output: Send,
{
    std::thread::scope(|s| {
        s.spawn(move || {
            tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("core block_on runtime")
                .block_on(f)
        })
        .join()
        .expect("core block_on thread")
    })
}

/// 给日志用的错误文本：去掉 TOML 解析诊断里引用的源码行（`1 | key = "..."` 和它上下的
/// `|`、`^` 标注行）。那一行是用户配置原文，出错的可能正是密钥那一行；行列号和原因留着。
/// （原 GUI lib.rs 的同名函数，核心版。）
pub fn error_for_log(error: &str) -> String {
    error
        .lines()
        .filter(|line| {
            !line
                .trim_start()
                .trim_start_matches(|c: char| c.is_ascii_digit())
                .trim_start()
                .starts_with('|')
        })
        .collect::<Vec<_>>()
        .join("\n")
}

const MIN_KNOWN_SECRET_LEN: usize = 8;

pub fn redact_known_secrets(text: &str, known_secrets: &[String]) -> String {
    redact_known_secrets_with_min_length(text, known_secrets, MIN_KNOWN_SECRET_LEN)
}

fn redact_known_secrets_with_min_length(
    text: &str,
    known_secrets: &[String],
    minimum_chars: usize,
) -> String {
    let mut output = text.to_string();
    for secret in known_secrets {
        if secret.chars().count() >= minimum_chars {
            output = output.replace(secret.as_str(), "[REDACTED]");
        }
    }
    output
}

/// 无 scheme 的裸 authority 形态(如 `user:pass@host/path`)剥掉 userinfo：
/// 仅当 `@` 出现在第一个 `/` 之前时才视为凭据。
fn strip_bare_userinfo(input: &str) -> &str {
    let authority_end = input.find('/').unwrap_or(input.len());
    match input[..authority_end].rfind('@') {
        Some(at) => &input[at + 1..],
        None => input,
    }
}

pub fn redact_url_for_log(url_str: &str) -> String {
    redact_url_for_log_with_secrets(url_str, &[])
}

/// 为日志脱敏 URL：剥掉 userinfo(user:pass@) 与整个 query/fragment，保留
/// scheme/host/port/path 供诊断(如 base_url 配错路径导致 404)，最后再抹掉已知密钥值。
pub fn redact_url_for_log_with_secrets(url_str: &str, known_secrets: &[String]) -> String {
    let scheme_relative = url_str.starts_with("//");
    let parsed = if scheme_relative {
        url::Url::parse(&format!("https:{url_str}"))
    } else {
        url::Url::parse(url_str)
    };

    let sanitized = match parsed {
        Ok(mut url) if url.has_host() => {
            let _ = url.set_username("");
            let _ = url.set_password(None);
            url.set_query(None);
            url.set_fragment(None);
            let rendered = url.as_str();
            if scheme_relative {
                rendered
                    .strip_prefix("https:")
                    .unwrap_or(rendered)
                    .to_string()
            } else {
                rendered.to_string()
            }
        }
        _ => {
            let without_tail = url_str.split(['?', '#']).next().unwrap_or(url_str);
            strip_bare_userinfo(without_tail).to_string()
        }
    };

    redact_known_secrets(&sanitized, known_secrets)
}

/// 只保留 `scheme://host:port`，丢掉 path/query/userinfo。用于我们手里没有任何
/// 已知密钥可脱敏 path 的场景——凭据可能整个内嵌在 base_url 的 path 里，此时
/// 记录 path 无法保证不泄漏，只能退回到 origin。
pub fn redact_url_origin_for_log(url_str: &str) -> String {
    let scheme_relative = url_str.starts_with("//");
    let parsed = if scheme_relative {
        url::Url::parse(&format!("https:{url_str}"))
    } else {
        url::Url::parse(url_str)
    };

    match parsed {
        Ok(url) if url.has_host() => {
            let authority = &url[url::Position::BeforeHost..url::Position::AfterPort];
            if scheme_relative {
                format!("//{authority}")
            } else {
                format!("{}://{authority}", url.scheme())
            }
        }
        _ => {
            let without_tail = url_str.split(['?', '#']).next().unwrap_or(url_str);
            strip_bare_userinfo(without_tail).to_string()
        }
    }
}

pub struct RedactedUrl<'a> {
    url: &'a str,
    known_secrets: &'a [String],
}

impl std::fmt::Display for RedactedUrl<'_> {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&redact_url_for_log_with_secrets(
            self.url,
            self.known_secrets,
        ))
    }
}

/// 为日志提供惰性 URL 脱敏包装；只有日志实际输出时才解析和重建 URL。
pub fn url_for_log(url: &str) -> RedactedUrl<'_> {
    RedactedUrl {
        url,
        known_secrets: &[],
    }
}

/// 为持有确切认证材料的调用方提供优先精确匹配、再启发式兜底的 URL 脱敏。
pub fn url_for_log_with_secrets<'a>(
    url: &'a str,
    known_secrets: &'a [String],
) -> RedactedUrl<'a> {
    RedactedUrl { url, known_secrets }
}

pub fn redact_known_secrets_strict(text: &str, known_secrets: &[String]) -> String {
    redact_known_secrets_with_min_length(text, known_secrets, 1)
}
