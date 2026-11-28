use cc_switch_core::app_config::{InstalledSkill, McpServer};
use cc_switch_core::prompt::Prompt;
use cc_switch_core::services::{
    LogFilters, McpService, PaginatedLogs, PromptService, SkillRepo, SkillService, UsageSummary,
};
use cc_switch_core::{AppState, AppType};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub(super) enum Page {
    #[default]
    Home,
    Providers,
    Mcp,
    Prompts,
    Skills,
    Discover,
    Repos,
    Proxy,
    Usage,
    Config,
    Settings,
}

impl Page {
    pub const NAV: [Self; 9] = [
        Self::Home,
        Self::Providers,
        Self::Mcp,
        Self::Prompts,
        Self::Skills,
        Self::Proxy,
        Self::Usage,
        Self::Config,
        Self::Settings,
    ];

    pub fn title(self) -> &'static str {
        match self {
            Self::Home => "首页",
            Self::Providers => "供应商",
            Self::Mcp => "MCP 服务器",
            Self::Prompts => "提示词",
            Self::Skills => "技能",
            Self::Discover => "技能 › 发现",
            Self::Repos => "技能 › 仓库",
            Self::Proxy => "代理",
            Self::Usage => "用量与日志",
            Self::Config => "配置与备份",
            Self::Settings => "CLI 设置",
        }
    }

    pub fn nav_index(self) -> usize {
        Self::NAV.iter().position(|p| *p == self).unwrap_or(4)
    }

    pub fn searchable(self) -> bool {
        matches!(
            self,
            Self::Providers
                | Self::Mcp
                | Self::Prompts
                | Self::Skills
                | Self::Discover
                | Self::Repos
        )
    }
}

#[derive(Default)]
pub(super) struct Data {
    pub mcp: Vec<McpServer>,
    pub prompts: Vec<Prompt>,
    pub skills: Vec<InstalledSkill>,
    pub repos: Vec<SkillRepo>,
    pub usage: Option<UsageSummary>,
    pub logs: Option<PaginatedLogs>,
    pub proxy: String,
    pub warnings: Vec<String>,
}

impl Data {
    // Each module fails independently: a corrupt MCP record must not hide providers.
    pub fn load(state: &AppState, app: &AppType, page: Page, log_page: u32) -> Self {
        let mut data = Self::default();
        macro_rules! load {
            ($field:ident, $label:expr, $value:expr) => {
                match $value {
                    Ok(value) => data.$field = value,
                    Err(error) => data.warnings.push(format!("{}：{error}", $label)),
                }
            };
        }
        load!(
            mcp,
            "MCP",
            McpService::get_all_servers(state).map(|v| v.into_values().collect())
        );
        if let Ok(unreadable) = McpService::list_unreadable(state) {
            if !unreadable.is_empty() {
                data.warnings.push(format!(
                    "{} 条 MCP 配置无法读取，请在桌面端修复",
                    unreadable.len()
                ));
            }
        }
        load!(
            prompts,
            "提示词",
            PromptService::get_prompts(state, app.clone()).map(|v| v.into_values().collect())
        );
        load!(skills, "技能", SkillService::get_all_installed(&state.db));
        load!(repos, "技能仓库", state.db.get_skill_repos());
        data.mcp.sort_by(|a, b| a.name.cmp(&b.name));
        data.prompts.sort_by(|a, b| a.name.cmp(&b.name));
        data.skills.sort_by(|a, b| a.name.cmp(&b.name));
        if page == Page::Usage {
            load!(
                usage,
                "用量",
                state
                    .db
                    .get_usage_summary(None, None, Some(app.as_str()), None, None)
                    .map(Some)
            );
            let filters = LogFilters {
                app_type: Some(app.as_str().into()),
                ..Default::default()
            };
            load!(
                logs,
                "请求日志",
                state.db.get_request_logs(&filters, log_page, 50).map(Some)
            );
        }
        if matches!(page, Page::Home | Page::Proxy) {
            data.proxy = if *app == AppType::Codex {
                super::actions::capture_cli(app, &["proxy".into(), "show".into()])
                    .unwrap_or_else(|e| format!("读取失败：{e}"))
            } else {
                "Antigravity 使用独立启动配置，不经过 CLI 代理。".into()
            };
        }
        data
    }
}

pub(super) fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    format!("tui-{nanos:x}-{:x}", NEXT.fetch_add(1, Ordering::Relaxed))
}
