//! MCP (Model Context Protocol) 服务器管理模块
//!
//! 本模块负责 MCP 服务器配置的验证、同步和导入导出。
//!
//! ## 模块结构
//!
//! - `validation` - 服务器配置验证
//! - `claude` - Claude MCP 同步和导入
//! - `codex` - Codex MCP 同步和导入（含 TOML 转换）
//! - `gemini` - Gemini MCP 同步和导入
//! - `opencode` - OpenCode MCP 同步和导入（含 local/remote 格式转换）
//! - `hermes` - Hermes MCP 同步和导入
//! - `omp` - Omp MCP 同步和导入

pub mod agy;
pub mod claude;
pub mod codex;
pub mod dsh;
pub mod opencode;
pub mod validation;

// 重新导出公共 API
pub use claude::{import_from_claude, remove_server_from_claude, sync_single_server_to_claude};
pub use codex::{import_from_codex, remove_server_from_codex, sync_single_server_to_codex};
pub use opencode::{
    import_from_opencode, remove_server_from_opencode, sync_single_server_to_opencode,
};

/// Imports can persist valid rows before encountering conflicts. Report the
/// skipped IDs to the caller without including connection strings or secrets.
pub(crate) fn import_outcome(
    count: usize,
    label: &str,
    skipped: Vec<String>,
) -> Result<usize, crate::error::AppError> {
    if skipped.is_empty() {
        Ok(count)
    } else {
        Err(crate::error::AppError::McpValidation(format!(
            "Imported {count} {label} MCP servers; skipped {}. Native configurations were preserved.",
            skipped.join("; ")
        )))
    }
}
