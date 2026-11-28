use std::process::Command;
use std::sync::Arc;
use std::time::Duration;

use cc_switch_core::app_config::{InstalledSkill, McpServer};
use cc_switch_core::prompt::{Prompt, PromptWriteExpectation};
use cc_switch_core::provider::Provider;
use cc_switch_core::services::{
    DiscoverableSkill, McpService, PromptService, SkillRepo, SkillService,
};
use cc_switch_core::{AppState, AppType, ProviderService};

use super::data::{Data, Page};
use super::form::{Form, Kind};
use super::provider::ProviderDraft;

#[derive(Clone)]
pub(super) enum Action {
    Refresh,
    SaveProvider(ProviderDraft),
    SaveForm(Form),
    DeleteProvider(Provider),
    ImportProvider,
    Cli(Vec<String>),
    DeleteMcp(McpServer),
    ToggleMcp(McpServer),
    ImportMcp,
    DeletePrompt(Prompt),
    TogglePrompt(Prompt),
    ImportPrompt,
    ToggleSkill(InstalledSkill),
    DeleteSkill(InstalledSkill),
    ReadSkill(InstalledSkill),
    SyncSkills,
    Discover,
    InstallSkill(DiscoverableSkill),
    UpdateSkill(InstalledSkill),
    DeleteRepo(SkillRepo),
    Backup,
}

impl Action {
    pub fn saves_form(&self) -> bool {
        matches!(self, Self::SaveProvider(_) | Self::SaveForm(_))
    }

    pub fn label(&self) -> &'static str {
        match self {
            Self::Refresh => "刷新数据",
            Self::SaveProvider(_) | Self::SaveForm(_) => "保存配置",
            Self::DeleteProvider(_)
            | Self::DeleteMcp(_)
            | Self::DeletePrompt(_)
            | Self::DeleteSkill(_)
            | Self::DeleteRepo(_) => "删除",
            Self::ImportProvider | Self::ImportMcp | Self::ImportPrompt => "导入本机配置",
            Self::Cli(_) => "执行 CLI 操作",
            Self::ToggleMcp(_) | Self::TogglePrompt(_) | Self::ToggleSkill(_) => "更新启用状态",
            Self::ReadSkill(_) => "读取技能文档",
            Self::SyncSkills => "同步技能",
            Self::Discover => "从仓库发现技能",
            Self::InstallSkill(_) => "下载并安装技能",
            Self::UpdateSkill(_) => "更新技能",
            Self::Backup => "创建数据库备份",
        }
    }
}

pub(super) struct Effect {
    pub message: String,
    pub discovered: Option<Vec<DiscoverableSkill>>,
    pub view: Option<(String, String)>,
}

impl Effect {
    fn message(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
            discovered: None,
            view: None,
        }
    }
}

pub(super) struct Completed {
    pub effect: Result<Effect, String>,
    pub data: Data,
    pub providers: Result<(Vec<Provider>, String), String>,
    pub saved: bool,
}

pub(super) fn load_providers(
    state: &AppState,
    app: &AppType,
) -> Result<(Vec<Provider>, String), String> {
    let mut providers: Vec<_> = ProviderService::list(state, app.clone())
        .map_err(|e| e.to_string())?
        .into_values()
        .collect();
    providers.sort_by(|a, b| {
        a.sort_index
            .unwrap_or(usize::MAX)
            .cmp(&b.sort_index.unwrap_or(usize::MAX))
            .then_with(|| a.name.cmp(&b.name))
    });
    let current = ProviderService::current(state, app.clone()).map_err(|e| e.to_string())?;
    Ok((providers, current))
}

pub(super) fn run(
    action: Action,
    state: Arc<AppState>,
    app: AppType,
    page: Page,
    log_page: u32,
) -> Completed {
    let saved = action.saves_form();
    let effect = execute(action, &state, &app);
    Completed {
        effect,
        data: Data::load(&state, &app, page, log_page),
        providers: load_providers(&state, &app),
        saved,
    }
}

fn same<T: serde::Serialize>(current: &Option<T>, expected: &T) -> Result<(), String> {
    if serde_json::to_value(current).unwrap() != serde_json::to_value(Some(expected)).unwrap() {
        return Err("此项目已被其他窗口修改或删除，请刷新后重试".into());
    }
    Ok(())
}

pub(super) fn execute(action: Action, state: &AppState, app: &AppType) -> Result<Effect, String> {
    macro_rules! call {
        ($value:expr) => {
            $value.map_err(|e| e.to_string())?
        };
    }
    let message = match action {
        Action::Refresh => {
            reload_settings()?;
            "已刷新".into()
        }
        Action::SaveProvider(draft) => {
            draft.save(state)?;
            "供应商已保存".into()
        }
        Action::SaveForm(form) => {
            if matches!(form.kind, Kind::Zip) {
                let path = form.fields[0].value.trim().trim_matches('"');
                if path.is_empty() {
                    return Err("请输入 ZIP 文件路径".into());
                }
                let report = call!(SkillService::install_from_zip(
                    &state.db,
                    std::path::Path::new(path),
                    app
                ));
                format!(
                    "已安装 {} 个技能，跳过 {} 个已有目录",
                    report.installed.len(),
                    report.skipped.len()
                )
            } else {
                form.save(state)?
            }
        }
        Action::DeleteProvider(provider) => {
            let _guard = super::super::lock_proxy_control(
                &super::super::app_dir().join("cli-control.lock"),
            )?;
            same(
                &call!(state.db.get_provider_by_id(&provider.id, app)),
                &provider,
            )?;
            call!(ProviderService::delete(state, app.clone(), &provider.id));
            "供应商已删除".into()
        }
        Action::ImportProvider => {
            let _guard = super::super::lock_proxy_control(
                &super::super::app_dir().join("cli-control.lock"),
            )?;
            if call!(ProviderService::import_default_config(state, app.clone())) {
                "已导入本机供应商".into()
            } else {
                "没有可导入的新供应商".into()
            }
        }
        Action::Cli(args) => {
            let result = capture_cli(app, &args);
            // The child can update the device-local current-provider pointer.
            // Refresh even after a partial failure so the header matches disk.
            reload_settings().map_err(|e| format!("命令已执行，但最新设置读取失败：{e}"))?;
            result?
        }
        Action::DeleteMcp(server) => {
            cc_switch_core::block_on(async {
                let _guards = McpService::lock_live_sync_apps(state).await;
                let current = call!(state.db.get_mcp_server_rows())
                    .get(&server.id)
                    .map_err(|e| e.to_string())?
                    .cloned();
                same(&current, &server)?;
                call!(McpService::delete_server(state, &server.id));
                Ok::<_, String>(())
            })?;
            "MCP 已从所有启用的应用中删除".into()
        }
        Action::ToggleMcp(server) => {
            call!(cc_switch_core::block_on(McpService::toggle_app_locked(
                state,
                &server.id,
                app.clone(),
                !server.apps.is_enabled_for(app)
            )));
            "MCP 启用状态已更新".into()
        }
        Action::ImportMcp => {
            let count = cc_switch_core::block_on(async {
                let _guards = McpService::lock_live_sync_apps(state).await;
                McpService::import_from_app(state, app)
            });
            format!("已导入 {} 个 MCP 服务器", call!(count))
        }
        Action::DeletePrompt(prompt) => {
            let current = call!(state.db.get_prompts(app)).get(&prompt.id).cloned();
            same(&current, &prompt)?;
            call!(PromptService::delete_prompt(state, app.clone(), &prompt.id));
            "提示词已删除".into()
        }
        Action::TogglePrompt(mut prompt) => {
            if prompt.enabled {
                // Refresh the active template from its native file before disabling.
                // An external edit since selection must not be silently cleared.
                let current = call!(PromptService::get_prompts(state, app.clone()))
                    .get(&prompt.id)
                    .cloned();
                same(&current, &prompt)?;
                let expected = PromptWriteExpectation {
                    prompt: Some(prompt.clone()),
                    file_content: call!(PromptService::get_current_file_content(app.clone())),
                };
                prompt.enabled = false;
                let id = prompt.id.clone();
                call!(PromptService::upsert_prompt_checked(
                    state,
                    app.clone(),
                    &id,
                    prompt,
                    Some(&expected)
                ));
            } else {
                call!(PromptService::enable_prompt(state, app.clone(), &prompt.id));
            }
            "提示词启用状态已更新".into()
        }
        Action::ImportPrompt => {
            call!(PromptService::import_from_file(state, app.clone()));
            "已导入本机提示词（未启用）".into()
        }
        Action::ToggleSkill(skill) => {
            call!(SkillService::toggle_app(
                &state.db,
                &skill.id,
                app,
                !skill.apps.is_enabled_for(app)
            ));
            "技能启用状态已更新".into()
        }
        Action::DeleteSkill(skill) => {
            let report = call!(SkillService::uninstall(&state.db, &skill.id));
            format!(
                "技能已卸载{}",
                report
                    .backup_path
                    .map(|p| format!("；本地修改备份：{p}"))
                    .unwrap_or_default()
            )
        }
        Action::ReadSkill(skill) => {
            let dir = call!(SkillService::get_installed_skill_dir(&skill.directory));
            let path = call!(cc_switch_core::prompt_files::checked_child_path(
                &dir,
                std::path::Path::new("SKILL.md")
            ));
            let content = call!(cc_switch_core::prompt_files::read_text_file(&path))
                .ok_or("找不到 SKILL.md")?;
            return Ok(Effect {
                message: "已读取技能文档".into(),
                discovered: None,
                view: Some((skill.name, content)),
            });
        }
        Action::SyncSkills => {
            call!(SkillService::sync_to_app(&state.db, app));
            "已同步当前应用的技能".into()
        }
        Action::Discover => {
            let repos = call!(state.db.get_skill_repos());
            let report = call!(cc_switch_core::block_on(async {
                SkillService::new()
                    .discover_available_report(repos, true)
                    .await
            }));
            let mut message = format!("发现 {} 个技能", report.skills.len());
            for failure in report.failures {
                message.push_str(&format!(
                    "\n{}/{}：{}",
                    failure.owner, failure.name, failure.error
                ));
            }
            return Ok(Effect {
                message,
                discovered: Some(report.skills),
                view: None,
            });
        }
        Action::InstallSkill(skill) => {
            call!(cc_switch_core::block_on(async {
                SkillService::new().install(&state.db, &skill, app).await
            }));
            "技能已安装并启用于当前应用".into()
        }
        Action::UpdateSkill(skill) => {
            call!(cc_switch_core::block_on(async {
                SkillService::new().update_skill(&state.db, &skill.id).await
            }));
            "技能已更新".into()
        }
        Action::DeleteRepo(repo) => {
            let _guard = cc_switch_core::services::skill::skill_state_write_guard();
            let current = call!(state.db.get_skill_repos())
                .into_iter()
                .find(|r| r.owner == repo.owner && r.name == repo.name);
            same(&current, &repo)?;
            call!(state.db.delete_skill_repo(&repo.owner, &repo.name));
            "仓库已移除，已安装的技能保留".into()
        }
        Action::Backup => match call!(state.db.backup_database_file()) {
            Some(path) => format!("备份已创建：{}", path.display()),
            None => "当前数据库无需文件备份".into(),
        },
    };
    Ok(Effect::message(message))
}

pub(super) fn cli_command(app: &AppType, args: &[String]) -> Result<Command, String> {
    let mut command = Command::new(std::env::current_exe().map_err(|e| e.to_string())?);
    command
        .arg("--app")
        .arg(app.as_str())
        .args(args)
        .env("CC_SWITCH_CONFIG_DIR", super::super::app_dir());
    Ok(command)
}

fn reload_settings() -> Result<(), String> {
    if super::super::app_dir().join("settings.json").exists() {
        cc_switch_core::settings::reload_settings().map_err(|e| e.to_string())?;
    }
    Ok(())
}

pub(super) fn capture_cli(app: &AppType, args: &[String]) -> Result<String, String> {
    let output = cc_switch_core::process_output::output_with_timeout(
        &mut cli_command(app, args)?,
        Duration::from_secs(45),
    )?;
    let mut text = String::from_utf8_lossy(&output.stdout).trim().to_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.trim().is_empty() {
        text.push('\n');
        text.push_str(stderr.trim());
    }
    if output.status.success() {
        Ok(text)
    } else {
        Err(if text.is_empty() {
            format!("命令退出：{}", output.status)
        } else {
            text
        })
    }
}
