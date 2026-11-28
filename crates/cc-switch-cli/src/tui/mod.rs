//! Interactive frontend. Persistence stays in shared core services.
mod actions;
mod data;
mod editor;
mod form;
mod keymap;
mod provider;
mod settings;
#[cfg(test)]
mod tests;
mod theme;
mod ui;

use actions::Action;
use cc_switch_core::provider::Provider;
use cc_switch_core::services::DiscoverableSkill;
use cc_switch_core::{frontend_apps, AppState, AppType};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use data::{Data, Page};
use editor::TextEditor;
use form::Form;
use keymap::Intent;
use provider::ProviderDraft;
use settings::SettingsDraft;
use std::io::{self, IsTerminal};
use std::sync::{mpsc, Arc};
use std::time::Duration;

#[derive(Clone, Copy)]
enum Discard {
    Quit,
    Provider,
    Field,
    Settings,
    Form,
}

enum Confirm {
    Discard(Discard),
    Action(String, Action),
}

struct Viewer {
    title: String,
    text: String,
    scroll: u16,
}

struct App {
    apps: Vec<AppType>,
    app_index: usize,
    providers: Vec<Provider>,
    selected: usize,
    current: String,
    page: Page,
    nav_index: usize,
    nav_focus: bool,
    data: Data,
    discovered: Vec<DiscoverableSkill>,
    filter: String,
    filtering: bool,
    log_page: u32,
    settings: Option<SettingsDraft>,
    draft: Option<ProviderDraft>,
    form: Option<Form>,
    editor: Option<TextEditor>,
    confirm: Option<Confirm>,
    viewer: Option<Viewer>,
    help: bool,
    help_scroll: u16,
    status: String,
    error: bool,
    pending: Option<Action>,
    busy: Option<String>,
    launch: Option<String>,
    quit: bool,
}

impl App {
    fn new(app: AppType) -> Self {
        let apps = frontend_apps::load_whitelist("cli");
        let app_index = apps.iter().position(|a| *a == app).unwrap_or(0);
        Self::with_apps(apps, app_index)
    }

    fn with_apps(apps: Vec<AppType>, app_index: usize) -> Self {
        Self {
            apps,
            app_index,
            providers: vec![],
            selected: 0,
            current: String::new(),
            page: Page::Home,
            nav_index: 0,
            nav_focus: true,
            data: Data::default(),
            discovered: vec![],
            filter: String::new(),
            filtering: false,
            log_page: 0,
            settings: None,
            draft: None,
            form: None,
            editor: None,
            confirm: None,
            viewer: None,
            help: false,
            help_scroll: 0,
            status: String::new(),
            error: false,
            pending: None,
            busy: None,
            launch: None,
            quit: false,
        }
    }

    fn app(&self) -> AppType {
        self.apps[self.app_index].clone()
    }

    fn load_settings(&mut self) -> Result<(), String> {
        self.settings = None;
        self.settings = Some(SettingsDraft::load(&super::cli_config_path())?);
        Ok(())
    }

    fn field_text(&self) -> String {
        if let Some(draft) = &self.draft {
            draft.field_text(draft.field)
        } else if let Some(form) = &self.form {
            form.fields[form.selected].value.clone()
        } else {
            String::new()
        }
    }

    fn field_dirty(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|e| e.text() != self.field_text())
    }

    fn dirty(&self) -> bool {
        self.field_dirty()
            || self.draft.as_ref().is_some_and(ProviderDraft::dirty)
            || self.form.as_ref().is_some_and(Form::dirty)
            || self.settings.as_ref().is_some_and(SettingsDraft::dirty)
    }

    fn discard(&mut self, action: Discard) -> Result<(), String> {
        match action {
            Discard::Quit => self.quit = true,
            Discard::Provider => self.draft = None,
            Discard::Form => self.form = None,
            Discard::Field => self.editor = None,
            Discard::Settings => self.load_settings()?,
        }
        Ok(())
    }

    fn ask_discard(&mut self, action: Discard, dirty: bool) -> Result<(), String> {
        if dirty {
            self.confirm = Some(Confirm::Discard(action));
        } else {
            self.discard(action)?;
        }
        Ok(())
    }

    fn queue(&mut self, action: Action) {
        self.error = false;
        self.pending = Some(action);
    }

    fn confirm_action(&mut self, text: impl Into<String>, action: Action) {
        self.confirm = Some(Confirm::Action(text.into(), action));
    }

    fn go(&mut self, page: Page) -> Result<(), String> {
        self.page = page;
        self.nav_index = page.nav_index();
        self.nav_focus = false;
        self.selected = 0;
        self.filter.clear();
        self.filtering = false;
        self.log_page = 0;
        if page == Page::Settings {
            if self.settings.is_none() {
                self.load_settings()?;
            }
        } else {
            self.queue(Action::Refresh);
        }
        Ok(())
    }

    // Selection indexes the filtered list, never the underlying data.
    fn visible(&self) -> Vec<usize> {
        let query = self.filter.to_lowercase();
        let matches = |name: &str, id: &str| {
            query.is_empty()
                || name.to_lowercase().contains(&query)
                || id.to_lowercase().contains(&query)
        };
        match self.page {
            Page::Providers => self
                .providers
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(&p.name, &p.id))
                .map(|(i, _)| i)
                .collect(),
            Page::Mcp => self
                .data
                .mcp
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(&p.name, &p.id))
                .map(|(i, _)| i)
                .collect(),
            Page::Prompts => self
                .data
                .prompts
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(&p.name, &p.id))
                .map(|(i, _)| i)
                .collect(),
            Page::Skills => self
                .data
                .skills
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(&p.name, &p.directory))
                .map(|(i, _)| i)
                .collect(),
            Page::Discover => self
                .discovered
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(&p.name, &p.key))
                .map(|(i, _)| i)
                .collect(),
            Page::Repos => self
                .data
                .repos
                .iter()
                .enumerate()
                .filter(|(_, p)| matches(&p.name, &p.owner))
                .map(|(i, _)| i)
                .collect(),
            Page::Usage => (0..self.data.logs.as_ref().map_or(0, |p| p.data.len())).collect(),
            _ => vec![],
        }
    }

    fn selected_index(&self) -> Option<usize> {
        self.visible().get(self.selected).copied()
    }

    fn key(&mut self, key: KeyEvent, state: &AppState) -> Result<(), String> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }
        if self.confirm.is_some() {
            match key.code {
                KeyCode::Char('y' | 'Y') => match self.confirm.take().unwrap() {
                    Confirm::Discard(action) => self.discard(action)?,
                    Confirm::Action(_, action) => self.queue(action),
                },
                KeyCode::Esc | KeyCode::Char('n' | 'N') => self.confirm = None,
                _ => {}
            }
            return Ok(());
        }
        if self.help || self.viewer.is_some() {
            let scroll = if self.help {
                &mut self.help_scroll
            } else {
                &mut self.viewer.as_mut().unwrap().scroll
            };
            match key.code {
                KeyCode::Down | KeyCode::Char('j') => *scroll = scroll.saturating_add(1),
                KeyCode::Up | KeyCode::Char('k') => *scroll = scroll.saturating_sub(1),
                KeyCode::PageDown => *scroll = scroll.saturating_add(10),
                KeyCode::PageUp => *scroll = scroll.saturating_sub(10),
                KeyCode::Home => *scroll = 0,
                KeyCode::Esc | KeyCode::Char('q' | '?') | KeyCode::F(1) | KeyCode::Enter => {
                    self.help = false;
                    self.viewer = None;
                }
                _ => {}
            }
            return Ok(());
        }
        if key.code == KeyCode::F(1)
            || (self.editor.is_none()
                && !self.filtering
                && matches!(key.code, KeyCode::Char('?' | '？')))
        {
            self.help = true;
            self.help_scroll = 0;
            return Ok(());
        }
        if !self.status.is_empty()
            && (key.code == KeyCode::F(4)
                || (self.editor.is_none() && !self.filtering && key.code == KeyCode::Char('!')))
        {
            self.viewer = Some(Viewer {
                title: "操作详情".into(),
                text: self.status.clone(),
                scroll: 0,
            });
            return Ok(());
        }
        if self.busy.is_some() || self.pending.is_some() {
            return Ok(());
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            return self.ask_discard(Discard::Quit, self.dirty());
        }
        let save = ctrl && key.code == KeyCode::Char('s');
        if self.editor.is_some() {
            if key.code == KeyCode::Esc {
                self.ask_discard(Discard::Field, self.field_dirty())?;
            } else if save
                || (!self.editor.as_ref().unwrap().multiline && key.code == KeyCode::Enter)
            {
                let text = self.editor.as_ref().unwrap().text();
                if let Some(draft) = &mut self.draft {
                    draft.apply_field(draft.field, &text)?;
                } else if let Some(form) = &mut self.form {
                    form.apply(text);
                }
                self.editor = None;
                self.status = "字段已更新到草稿；Ctrl+S 保存".into();
                self.error = false;
            } else {
                self.editor.as_mut().unwrap().key(key);
            }
            return Ok(());
        }
        if let Some(draft) = &mut self.draft {
            match key.code {
                KeyCode::Esc => {
                    let dirty = draft.dirty();
                    self.ask_discard(Discard::Provider, dirty)?;
                }
                _ if save => {
                    let draft = draft.clone();
                    self.queue(Action::SaveProvider(draft));
                }
                KeyCode::Up | KeyCode::BackTab => draft.field = draft.field.saturating_sub(1),
                KeyCode::Down | KeyCode::Tab => {
                    draft.field = (draft.field + 1) % draft.field_count()
                }
                KeyCode::Enter | KeyCode::Char('e') => {
                    self.editor = Some(TextEditor::new(
                        &draft.field_text(draft.field),
                        draft.field >= 4,
                        draft.field == 2,
                    ));
                    self.status.clear();
                }
                _ => {}
            }
            return Ok(());
        }
        if let Some(form) = &mut self.form {
            match key.code {
                KeyCode::Esc => {
                    let dirty = form.dirty();
                    self.ask_discard(Discard::Form, dirty)?;
                }
                _ if save => {
                    let form = form.clone();
                    self.queue(Action::SaveForm(form));
                }
                KeyCode::Up | KeyCode::BackTab => form.selected = form.selected.saturating_sub(1),
                KeyCode::Down | KeyCode::Tab => {
                    form.selected = (form.selected + 1) % form.fields.len()
                }
                KeyCode::Enter | KeyCode::Char('e' | ' ') => {
                    let field = &mut form.fields[form.selected];
                    if field.readonly {
                        return Ok(());
                    }
                    if field.toggle {
                        field.value = (field.value != "true").to_string();
                    } else {
                        self.editor = Some(TextEditor::new(&field.value, field.multiline, false));
                        self.status.clear();
                    }
                }
                _ => {}
            }
            return Ok(());
        }
        if self.filtering {
            match key.code {
                KeyCode::Esc => {
                    self.filter.clear();
                    self.filtering = false;
                }
                KeyCode::Enter => self.filtering = false,
                KeyCode::Backspace => {
                    self.filter.pop();
                }
                KeyCode::Char(c) if !ctrl && !key.modifiers.contains(KeyModifiers::ALT) => {
                    self.filter.push(c)
                }
                _ => {}
            }
            self.selected = 0;
            return Ok(());
        }
        match key.code {
            KeyCode::Char('q') => return self.ask_discard(Discard::Quit, self.dirty()),
            KeyCode::Char('!') if !self.status.is_empty() => {
                self.viewer = Some(Viewer {
                    title: "操作详情".into(),
                    text: self.status.clone(),
                    scroll: 0,
                });
                return Ok(());
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.nav_focus = !self.nav_focus;
                return Ok(());
            }
            KeyCode::Left => {
                self.nav_focus = true;
                return Ok(());
            }
            KeyCode::Right => {
                self.nav_focus = false;
                return Ok(());
            }
            KeyCode::Esc if !self.filter.is_empty() => {
                self.filter.clear();
                self.selected = 0;
                return Ok(());
            }
            KeyCode::Esc if matches!(self.page, Page::Discover | Page::Repos) => {
                return self.go(Page::Skills)
            }
            KeyCode::Esc => {
                self.nav_focus = true;
                return Ok(());
            }
            KeyCode::Char('[' | ']') => {
                self.app_index = if key.code == KeyCode::Char(']') {
                    (self.app_index + 1) % self.apps.len()
                } else {
                    (self.app_index + self.apps.len() - 1) % self.apps.len()
                };
                self.providers.clear();
                self.current.clear();
                self.data = Data::default();
                self.selected = 0;
                self.filter.clear();
                self.log_page = 0;
                self.queue(Action::Refresh);
                return Ok(());
            }
            KeyCode::Char(c @ '1'..='9') => return self.go(Page::NAV[(c as u8 - b'1') as usize]),
            _ => {}
        }
        if self.nav_focus {
            match key.code {
                KeyCode::Up | KeyCode::Char('k') => {
                    self.nav_index = self.nav_index.saturating_sub(1)
                }
                KeyCode::Down | KeyCode::Char('j') => {
                    self.nav_index = (self.nav_index + 1) % Page::NAV.len()
                }
                KeyCode::Enter => self.go(Page::NAV[self.nav_index])?,
                _ => {}
            }
            return Ok(());
        }
        if self.page == Page::Settings {
            if key.code == KeyCode::Char('r') {
                self.ask_discard(
                    Discard::Settings,
                    self.settings.as_ref().is_some_and(SettingsDraft::dirty),
                )?;
            } else if let Some(settings) = &mut self.settings {
                match key.code {
                    _ if save => {
                        settings.save(&super::cli_config_path())?;
                        self.status = "CLI 设置已保存；重启后台代理后生效".into();
                        self.error = false;
                        self.load_settings()?;
                    }
                    KeyCode::Up | KeyCode::Down | KeyCode::Char('j' | 'k') => {
                        settings.selected = 1 - settings.selected
                    }
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        settings.values[settings.selected] = !settings.values[settings.selected]
                    }
                    _ => {}
                }
            }
            return Ok(());
        }
        match key.code {
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.visible().len().saturating_sub(1))
            }
            KeyCode::Home => self.selected = 0,
            KeyCode::End => self.selected = self.visible().len().saturating_sub(1),
            KeyCode::PageUp => self.selected = self.selected.saturating_sub(10),
            KeyCode::PageDown => {
                self.selected = (self.selected + 10).min(self.visible().len().saturating_sub(1))
            }
            _ => {
                if let Some(binding) = keymap::bindings(self.page, &self.app())
                    .into_iter()
                    .find(|b| b.key == key.code)
                {
                    self.intent(binding.intent, state)?;
                }
            }
        }
        Ok(())
    }

    fn intent(&mut self, intent: Intent, state: &AppState) -> Result<(), String> {
        use Intent::*;
        let index = self.selected_index();
        match intent {
            Search => {
                self.filtering = true;
                return Ok(());
            }
            Refresh => {
                self.queue(if self.page == Page::Discover {
                    Action::Discover
                } else {
                    Action::Refresh
                });
                return Ok(());
            }
            Providers => return self.go(Page::Providers),
            Repos => return self.go(Page::Repos),
            Discover => {
                self.go(Page::Discover)?;
                self.queue(Action::Discover);
                return Ok(());
            }
            Zip => {
                self.form = Some(Form::zip());
                return Ok(());
            }
            Backup => {
                self.queue(Action::Backup);
                return Ok(());
            }
            Sync => {
                self.queue(Action::SyncSkills);
                return Ok(());
            }
            StartProxy | StopProxy => {
                self.confirm_action(
                    if intent == StartProxy {
                        "启动 CLI 后台代理，并按当前路由更新客户端配置？"
                    } else {
                        "停止 CLI 后台代理，并将客户端配置恢复为直连？"
                    },
                    Action::Cli(vec![
                        "proxy".into(),
                        if intent == StartProxy {
                            "enable"
                        } else {
                            "disable"
                        }
                        .into(),
                    ]),
                );
                return Ok(());
            }
            PreviousPage | NextPage => {
                let total = self.data.logs.as_ref().map_or(0, |l| l.total);
                self.log_page = if intent == PreviousPage {
                    self.log_page.saturating_sub(1)
                } else {
                    (self.log_page + 1).min(total.saturating_sub(1) / 50)
                };
                self.selected = 0;
                self.queue(Action::Refresh);
                return Ok(());
            }
            _ => {}
        }
        match self.page {
            Page::Providers => match intent {
                New => self.draft = Some(ProviderDraft::create(state, self.app(), None)?),
                Import => {
                    self.confirm_action("从当前应用的本机配置导入供应商？", Action::ImportProvider)
                }
                _ => {
                    if let Some(index) = index {
                        let provider = self.providers[index].clone();
                        match intent {
                            Edit => {
                                self.draft =
                                    Some(ProviderDraft::open(state, self.app(), &provider.id)?)
                            }
                            Copy => {
                                self.draft =
                                    Some(ProviderDraft::create(state, self.app(), Some(&provider))?)
                            }
                            Delete => self.confirm_action(
                                format!("删除供应商「{}」？", provider.name),
                                Action::DeleteProvider(provider),
                            ),
                            Activate if self.app() == AppType::Agy => {
                                self.launch = Some(provider.id)
                            }
                            Activate => self.confirm_action(
                                format!("切换到「{}」并更新 Codex 配置？", provider.name),
                                Action::Cli(vec!["provider".into(), "switch".into(), provider.id]),
                            ),
                            _ => {}
                        }
                    }
                }
            },
            Page::Mcp => match intent {
                New => self.form = Some(Form::mcp(None, self.app())),
                Import => self.confirm_action("从当前应用导入 MCP 服务器？", Action::ImportMcp),
                _ => {
                    if let Some(index) = index {
                        let server = self.data.mcp[index].clone();
                        match intent {
                            Edit => self.form = Some(Form::mcp(Some(server), self.app())),
                            Activate => self.queue(Action::ToggleMcp(server)),
                            Delete => self.confirm_action(
                                format!("删除「{}」？将从所有已启用的应用中移除。", server.name),
                                Action::DeleteMcp(server),
                            ),
                            _ => {}
                        }
                    }
                }
            },
            Page::Prompts => match intent {
                New => self.form = Some(Form::prompt(None, self.app())?),
                Import => self.queue(Action::ImportPrompt),
                _ => {
                    if let Some(index) = index {
                        let prompt = self.data.prompts[index].clone();
                        match intent {
                            Edit => self.form = Some(Form::prompt(Some(prompt), self.app())?),
                            Activate => self.confirm_action(
                                format!(
                                    "{}提示词「{}」并更新当前应用的提示词文件？",
                                    if prompt.enabled { "停用" } else { "启用" },
                                    prompt.name
                                ),
                                Action::TogglePrompt(prompt),
                            ),
                            Delete => self.confirm_action(
                                format!("删除提示词「{}」？已启用的提示词须先停用。", prompt.name),
                                Action::DeletePrompt(prompt),
                            ),
                            _ => {}
                        }
                    }
                }
            },
            Page::Skills => {
                if let Some(index) = index {
                    let skill = self.data.skills[index].clone();
                    match intent {
                        Edit => self.queue(Action::ReadSkill(skill)),
                        Activate => self.queue(Action::ToggleSkill(skill)),
                        Update => self.confirm_action(
                            format!("从仓库更新「{}」？本地修改由核心服务备份。", skill.name),
                            Action::UpdateSkill(skill),
                        ),
                        Delete => self.confirm_action(
                            format!("卸载「{}」？将从所有应用中移除。", skill.name),
                            Action::DeleteSkill(skill),
                        ),
                        _ => {}
                    }
                }
            }
            Page::Discover => {
                if let Some(index) = index {
                    let skill = self.discovered[index].clone();
                    self.confirm_action(
                        format!(
                            "安装「{}」并启用于 {}？\n来源：{}/{} ({})",
                            skill.name,
                            self.app().as_str(),
                            skill.repo_owner,
                            skill.repo_name,
                            skill.repo_branch
                        ),
                        Action::InstallSkill(skill),
                    );
                }
            }
            Page::Repos => match intent {
                New => self.form = Some(Form::repo(None)),
                _ => {
                    if let Some(index) = index {
                        let repo = self.data.repos[index].clone();
                        if intent == Edit {
                            self.form = Some(Form::repo(Some(repo)));
                        } else if intent == Delete {
                            self.confirm_action(
                                format!(
                                    "移除仓库 {}/{}？已安装的技能保留。",
                                    repo.owner, repo.name
                                ),
                                Action::DeleteRepo(repo),
                            );
                        }
                    }
                }
            },
            Page::Usage => {
                if let Some(index) = index {
                    let log = &self.data.logs.as_ref().unwrap().data[index];
                    self.viewer = Some(Viewer {
                        title: "请求详情".into(),
                        text: serde_json::to_string_pretty(log).unwrap_or_default(),
                        scroll: 0,
                    });
                }
            }
            _ => {}
        }
        Ok(())
    }

    fn complete(&mut self, completed: actions::Completed) {
        self.busy = None;
        self.data = completed.data;
        match completed.providers {
            Ok((providers, current)) => {
                self.providers = providers;
                self.current = current;
            }
            Err(error) => self.data.warnings.push(error),
        }
        match completed.effect {
            Ok(effect) => {
                self.error = false;
                self.status = effect.message;
                if let Some(skills) = effect.discovered {
                    self.discovered = skills;
                    self.selected = 0;
                }
                if let Some((title, text)) = effect.view {
                    self.viewer = Some(Viewer {
                        title,
                        text,
                        scroll: 0,
                    });
                }
                if completed.saved {
                    self.draft = None;
                    self.form = None;
                }
            }
            Err(error) => {
                self.error = true;
                self.status = cc_switch_core::error_for_log(&error);
            }
        }
        if !self.data.warnings.is_empty() {
            self.status.push('\n');
            self.status.push_str(&self.data.warnings.join("\n"));
            self.error = true;
        }
        self.selected = self.selected.min(self.visible().len().saturating_sub(1));
    }
}

struct TerminalGuard;
impl Drop for TerminalGuard {
    fn drop(&mut self) {
        let _ = crossterm::execute!(io::stdout(), DisableBracketedPaste, crossterm::cursor::Show);
        ratatui::restore();
    }
}

pub(super) fn run(app: AppType) -> Result<(), String> {
    if !io::stdin().is_terminal() || !io::stdout().is_terminal() {
        return Err("tui 需要交互式终端，请在终端直接运行 cc-switch-cli tui".into());
    }
    let state = super::try_open_state()?;
    state
        .proxy_service
        .set_hooks(cc_switch_core::events::FrontendHooks::with_state(
            Arc::new(super::QuietEventSink),
            Arc::new(super::CliOAuthStateProvider(Arc::downgrade(&state))),
            &state,
        ));
    let mut app = App::new(app);
    if app.apps.is_empty() {
        return Err("CLI 应用白名单为空".into());
    }
    app.queue(Action::Refresh);
    let _restore = TerminalGuard;
    let mut terminal = ratatui::try_init().map_err(|e| e.to_string())?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(io::stdout(), DisableBracketedPaste, crossterm::cursor::Show);
        previous_hook(info);
    }));
    crossterm::execute!(io::stdout(), EnableBracketedPaste).map_err(|e| e.to_string())?;
    let mut worker: Option<mpsc::Receiver<actions::Completed>> = None;
    let mut redraw = true;
    loop {
        if app.quit {
            break;
        }
        if let Some(action) = app.pending.take() {
            let (send, recv) = mpsc::channel();
            let state = Arc::clone(&state);
            let app_type = app.app();
            let page = app.page;
            let log_page = app.log_page;
            app.busy = Some(action.label().into());
            std::thread::spawn(move || {
                let _ = send.send(actions::run(action, state, app_type, page, log_page));
            });
            worker = Some(recv);
            redraw = true;
        }
        if let Some(recv) = &worker {
            match recv.try_recv() {
                Ok(completed) => {
                    app.complete(completed);
                    worker = None;
                    redraw = true;
                }
                Err(mpsc::TryRecvError::Disconnected) => {
                    app.busy = None;
                    app.error = true;
                    app.status = "后台任务异常结束，请刷新后检查实际状态".into();
                    worker = None;
                    redraw = true;
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
        }
        if let Some(id) = app.launch.take() {
            crossterm::execute!(io::stdout(), DisableBracketedPaste).map_err(|e| e.to_string())?;
            ratatui::restore();
            let result = actions::cli_command(&app.app(), &["launch".into(), id, "--exec".into()])
                .and_then(|mut cmd| cmd.status().map_err(|e| e.to_string()));
            terminal = ratatui::try_init().map_err(|e| e.to_string())?;
            crossterm::execute!(io::stdout(), EnableBracketedPaste).map_err(|e| e.to_string())?;
            app.status = match result {
                Ok(status) => format!("Antigravity 已返回：{status}"),
                Err(error) => error,
            };
            redraw = true;
        }
        if redraw {
            terminal
                .draw(|frame| ui::draw(frame, &app))
                .map_err(|e| e.to_string())?;
            redraw = false;
        }
        if !event::poll(Duration::from_millis(120)).map_err(|e| e.to_string())? {
            continue;
        }
        redraw = true;
        match event::read().map_err(|e| e.to_string())? {
            Event::Key(key) => {
                if let Err(error) = app.key(key, &state) {
                    app.status = cc_switch_core::error_for_log(&error);
                    app.error = true;
                }
            }
            Event::Paste(text)
                if app.confirm.is_none()
                    && !app.help
                    && app.viewer.is_none()
                    && app.busy.is_none()
                    && app.pending.is_none() =>
            {
                if let Some(editor) = &mut app.editor {
                    editor.insert(&text);
                } else if app.filtering {
                    app.filter.extend(text.chars().filter(|c| !c.is_control()));
                    app.selected = 0;
                }
            }
            _ => {}
        }
    }
    Ok(())
}
