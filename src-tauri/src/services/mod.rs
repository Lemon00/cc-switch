//! GUI 侧的 services 门面：GUI 自有的服务留在这里，后端服务从
//! cc-switch-core 重导出。GUI 代码的 `crate::services::X` 路径不用改。

// GUI 自有服务（未进核心）
pub mod agy_account;
pub mod backup_storage;
pub mod balance;
pub mod codex_session_compression;
pub mod config;
pub mod dsh_account;
pub mod env_checker;
pub mod env_manager;
pub mod mcp_connectivity;
pub(crate) mod pi_state;
pub mod profile;
pub mod speedtest;
pub mod stream_check;

// 后端服务（核心）
pub use cc_switch_core::services::claude_account;
pub use cc_switch_core::services::codex_oauth_models;
pub use cc_switch_core::services::coding_plan;
pub use cc_switch_core::services::exchange_rates;
pub use cc_switch_core::services::kimi_oauth;
pub use cc_switch_core::services::mcp;
pub use cc_switch_core::services::model_fetch;
pub use cc_switch_core::services::model_pricing;
pub use cc_switch_core::services::omo;
pub use cc_switch_core::services::pi_prompt_files;
pub use cc_switch_core::services::prompt;
pub use cc_switch_core::services::prompt::PromptService;
pub use cc_switch_core::services::provider;
pub use cc_switch_core::services::proxy;
pub use cc_switch_core::services::s3;
pub use cc_switch_core::services::s3_auto_sync;
pub use cc_switch_core::services::s3_sync;
pub use cc_switch_core::services::session_common;
#[cfg(feature = "session-monitor")]
// 核心同名模块受 session-monitor 门控，shim 也必须跟着门控，
// 否则关掉 feature 时这里会重导出不存在的模块。
pub use cc_switch_core::services::session_usage;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_agy;
pub use cc_switch_core::services::session_usage_codex;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_dsh;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_gemini;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_grokbuild;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_kimi;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_mcode;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_omp;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_opencode;
#[cfg(feature = "session-monitor")]
pub use cc_switch_core::services::session_usage_pi;
pub use cc_switch_core::services::skill;
pub use cc_switch_core::services::sql_helpers;
pub use cc_switch_core::services::subscription;
pub use cc_switch_core::services::subscription_agy;
pub use cc_switch_core::services::subscription_grok;
pub use cc_switch_core::services::sync_protocol;
pub use cc_switch_core::services::usage_cache;
pub use cc_switch_core::services::usage_stats;
pub use cc_switch_core::services::webdav;
pub use cc_switch_core::services::webdav_auto_sync;
pub use cc_switch_core::services::webdav_sync;

pub use config::ConfigService;
pub use mcp::McpService;
pub use omo::OmoService;
pub use provider::{ProviderService, ProviderSortUpdate, SwitchResult};
pub use proxy::ProxyService;
#[allow(unused_imports)]
pub use skill::{DiscoverableSkill, Skill, SkillRepo, SkillService};
pub use speedtest::{EndpointLatency, SpeedtestService};
pub use usage_cache::UsageCache;
#[allow(unused_imports)]
pub use usage_stats::{
    DailyStats, LogFilters, ModelStats, PaginatedLogs, ProviderLimitStatus, ProviderStats,
    RequestLogDetail, UsageSummary, UsageSummaryByApp,
};
