//! Supported client identities shared by persistence and services.
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AppType {
    Claude,
    Codex,
    OpenCode,
    Dsh,
    Agy,
}

impl AppType {
    pub const SUPPORTED: [Self; 5] = [
        Self::Claude,
        Self::Codex,
        Self::OpenCode,
        Self::Dsh,
        Self::Agy,
    ];
    pub fn as_str(&self) -> &str {
        match self {
            Self::Claude => "claude",
            Self::Codex => "codex",
            Self::OpenCode => "opencode",
            Self::Dsh => "dsh",
            Self::Agy => "agy",
        }
    }
    pub fn is_additive_mode(&self) -> bool {
        matches!(self, Self::OpenCode | Self::Dsh)
    }
    pub fn supports_local_proxy(&self) -> bool {
        matches!(self, Self::Claude | Self::Codex)
    }
    pub fn has_route_mode(&self) -> bool {
        self.supports_local_proxy()
    }
    pub fn supports_stack(&self) -> bool {
        self.supports_local_proxy()
    }
    pub fn supports_mcp(&self) -> bool {
        true
    }
    pub fn has_native_provider_catalog(&self) -> bool {
        matches!(self, Self::Dsh)
    }
    pub fn is_supported(&self) -> bool {
        true
    }
    pub fn all() -> impl Iterator<Item = Self> {
        Self::SUPPORTED.into_iter()
    }
    pub fn persisted() -> impl Iterator<Item = Self> {
        Self::all()
    }

    /// Historical identifiers are recognized only for skipping old persistence rows.
    pub fn is_retired(value: &str) -> bool {
        matches!(
            value.trim().to_lowercase().as_str(),
            "claude-desktop"
                | "claude_desktop"
                | "claudedesktop"
                | "gemini"
                | "grokbuild"
                | "grok-build"
                | "grok_build"
                | "grok"
                | "openclaw"
                | "hermes"
                | "pi"
                | "mcode"
                | "kimi"
                | "kimi-code"
                | "omp"
                | "oh-my-pi"
                | "oh_my_pi"
                | "ohmypi"
        )
    }
}
impl std::fmt::Display for AppType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}
impl FromStr for AppType {
    type Err = AppError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_lowercase().as_str() {
            "claude" => Ok(Self::Claude),
            "codex" => Ok(Self::Codex),
            "opencode" => Ok(Self::OpenCode),
            "dsh" | "deepseek-harness" => Ok(Self::Dsh),
            "agy" | "antigravity" | "antigravity-cli" => Ok(Self::Agy),
            other => Err(AppError::Message(format!(
                "不支持的应用标识: '{other}'。可选值: claude, codex, opencode, dsh, agy。"
            ))),
        }
    }
}
