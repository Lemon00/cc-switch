//! Terminal configuration editor. Persistence stays in the shared core services.
mod editor;
mod provider;
mod settings;
#[cfg(test)]
mod tests;
mod ui;

use std::io::{self, IsTerminal};
use std::sync::Arc;

use cc_switch_core::provider::Provider;
use cc_switch_core::{frontend_apps, AppState, AppType, ProviderService};
use crossterm::event::{
    self, DisableBracketedPaste, EnableBracketedPaste, Event, KeyCode, KeyEvent, KeyEventKind,
    KeyModifiers,
};
use editor::TextEditor;
use provider::ProviderDraft;
use settings::SettingsDraft;

#[derive(Clone, Copy)]
enum Discard {
    Quit,
    Provider,
    Field,
    Settings,
}

struct App {
    apps: Vec<AppType>,
    app_index: usize,
    providers: Vec<Provider>,
    selected: usize,
    current: String,
    settings_page: bool,
    settings: Option<SettingsDraft>,
    draft: Option<ProviderDraft>,
    editor: Option<TextEditor>,
    confirm: Option<Discard>,
    help: bool,
    status: String,
    quit: bool,
}

impl App {
    fn new(app: AppType) -> Self {
        let apps = frontend_apps::load_whitelist("cli");
        let app_index = apps.iter().position(|a| *a == app).unwrap_or(0);
        Self {
            apps,
            app_index,
            providers: Vec::new(),
            selected: 0,
            current: String::new(),
            settings_page: false,
            settings: None,
            draft: None,
            editor: None,
            confirm: None,
            help: false,
            status: String::new(),
            quit: false,
        }
    }

    fn app(&self) -> AppType {
        self.apps[self.app_index].clone()
    }

    fn refresh(&mut self, state: &AppState) -> Result<(), String> {
        let selected_id = self.providers.get(self.selected).map(|p| p.id.clone());
        let mut providers: Vec<_> = ProviderService::list(state, self.app())
            .map_err(|e| e.to_string())?
            .into_values()
            .collect();
        providers.sort_by(|a, b| {
            a.sort_index
                .unwrap_or(usize::MAX)
                .cmp(&b.sort_index.unwrap_or(usize::MAX))
                .then_with(|| a.name.cmp(&b.name))
        });
        let current = ProviderService::current(state, self.app()).map_err(|e| e.to_string())?;
        self.selected = providers
            .iter()
            .position(|p| Some(&p.id) == selected_id.as_ref())
            .unwrap_or(self.selected.min(providers.len().saturating_sub(1)));
        self.providers = providers;
        self.current = current;
        Ok(())
    }

    fn load_settings(&mut self) -> Result<(), String> {
        // A failed read disables editing; it must never become a writable default.
        self.settings = None;
        self.settings = Some(SettingsDraft::load(&super::cli_config_path())?);
        Ok(())
    }

    fn field_dirty(&self) -> bool {
        match (&self.editor, &self.draft) {
            (Some(editor), Some(draft)) => editor.text() != draft.field_text(draft.field),
            _ => false,
        }
    }

    fn dirty(&self) -> bool {
        self.field_dirty()
            || self.draft.as_ref().is_some_and(ProviderDraft::dirty)
            || self.settings.as_ref().is_some_and(SettingsDraft::dirty)
    }

    fn discard(&mut self, action: Discard) -> Result<(), String> {
        match action {
            Discard::Quit => self.quit = true,
            Discard::Provider => self.draft = None,
            Discard::Field => self.editor = None,
            Discard::Settings => self.load_settings()?,
        }
        Ok(())
    }

    fn key(&mut self, key: KeyEvent, state: &AppState) -> Result<(), String> {
        if key.kind == KeyEventKind::Release {
            return Ok(());
        }
        if let Some(action) = self.confirm {
            match key.code {
                KeyCode::Char('y' | 'Y') => {
                    self.confirm = None;
                    self.discard(action)?;
                }
                KeyCode::Esc | KeyCode::Char('n' | 'N') => self.confirm = None,
                _ => {}
            }
            return Ok(());
        }
        if self.help {
            self.help = false;
            return Ok(());
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        if ctrl && key.code == KeyCode::Char('c') {
            if self.dirty() {
                self.confirm = Some(Discard::Quit);
            } else {
                self.quit = true;
            }
            return Ok(());
        }
        if key.code == KeyCode::F(1) || (self.editor.is_none() && key.code == KeyCode::Char('?')) {
            self.help = true;
            return Ok(());
        }
        let save = ctrl && key.code == KeyCode::Char('s');
        if let Some(editor) = &mut self.editor {
            if key.code == KeyCode::Esc {
                if self.field_dirty() {
                    self.confirm = Some(Discard::Field);
                } else {
                    self.editor = None;
                }
            } else if save || (!editor.multiline && key.code == KeyCode::Enter) {
                let text = editor.text();
                let draft = self
                    .draft
                    .as_mut()
                    .expect("editor belongs to provider draft");
                draft.apply_field(draft.field, &text)?;
                self.editor = None;
                self.status = "字段已更新到草稿；Ctrl+S 保存供应商".into();
            } else {
                editor.key(key);
            }
            return Ok(());
        }
        if let Some(draft) = &mut self.draft {
            match key.code {
                KeyCode::Esc => {
                    if draft.dirty() {
                        self.confirm = Some(Discard::Provider);
                    } else {
                        self.draft = None;
                    }
                }
                _ if save => {
                    draft.save(state)?;
                    self.draft = None;
                    self.status = if self.app() == AppType::Agy {
                        "供应商已保存；下次 launch 使用新配置".into()
                    } else {
                        "供应商已保存；已运行的 Codex 请重新启动以读取配置".into()
                    };
                    if let Err(error) = self.refresh(state) {
                        self.status.push_str(&format!("；列表刷新失败：{error}"));
                    }
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
        match key.code {
            KeyCode::Char('q') | KeyCode::Esc => {
                if self.dirty() {
                    self.confirm = Some(Discard::Quit);
                } else {
                    self.quit = true;
                }
            }
            KeyCode::Tab | KeyCode::BackTab => {
                self.settings_page = !self.settings_page;
                self.status.clear();
                if self.settings_page && self.settings.is_none() {
                    self.load_settings()?;
                }
            }
            KeyCode::Char('r') => {
                if self.settings_page {
                    if self.settings.as_ref().is_some_and(SettingsDraft::dirty) {
                        self.confirm = Some(Discard::Settings);
                    } else {
                        self.load_settings()?;
                    }
                } else {
                    self.refresh(state)?;
                }
            }
            _ if self.settings_page => {
                let Some(settings) = &mut self.settings else {
                    return Ok(());
                };
                match key.code {
                    _ if save => {
                        settings.save(&super::cli_config_path())?;
                        self.status =
                            "CLI 设置已保存；已运行的后台代理需 proxy disable 后重新 proxy enable"
                                .into();
                        self.load_settings()?;
                    }
                    KeyCode::Up | KeyCode::Down => settings.selected = 1 - settings.selected,
                    KeyCode::Enter | KeyCode::Char(' ') => {
                        settings.values[settings.selected] = !settings.values[settings.selected];
                    }
                    _ => {}
                }
            }
            KeyCode::Char('[' | ']') => {
                let next = if key.code == KeyCode::Char(']') {
                    (self.app_index + 1) % self.apps.len()
                } else {
                    (self.app_index + self.apps.len() - 1) % self.apps.len()
                };
                let previous = self.app_index;
                self.app_index = next;
                if let Err(error) = self.refresh(state) {
                    self.app_index = previous;
                    return Err(error);
                }
                self.status.clear();
            }
            KeyCode::Up | KeyCode::Char('k') => self.selected = self.selected.saturating_sub(1),
            KeyCode::Down | KeyCode::Char('j') => {
                self.selected = (self.selected + 1).min(self.providers.len().saturating_sub(1))
            }
            KeyCode::Enter | KeyCode::Char('e') => {
                if let Some(provider) = self.providers.get(self.selected) {
                    self.draft = Some(ProviderDraft::open(state, self.app(), &provider.id)?);
                    self.status.clear();
                }
            }
            _ => {}
        }
        Ok(())
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
    app.refresh(&state)?;
    let _restore = TerminalGuard;
    let mut terminal = ratatui::try_init().map_err(|e| e.to_string())?;
    let previous_hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = crossterm::execute!(io::stdout(), DisableBracketedPaste, crossterm::cursor::Show);
        previous_hook(info);
    }));
    crossterm::execute!(io::stdout(), EnableBracketedPaste).map_err(|e| e.to_string())?;
    while !app.quit {
        terminal
            .draw(|frame| ui::draw(frame, &app))
            .map_err(|e| e.to_string())?;
        match event::read().map_err(|e| e.to_string())? {
            Event::Key(key) => {
                if let Err(error) = app.key(key, &state) {
                    app.status = format!("保存/操作失败：{error}");
                }
            }
            Event::Paste(text) if app.confirm.is_none() && !app.help => {
                if let Some(editor) = &mut app.editor {
                    editor.insert(&text);
                }
            }
            _ => {}
        }
    }
    Ok(())
}
