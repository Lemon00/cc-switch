use super::*;
use cc_switch_core::{config, Database, ProviderService};
use ratatui::{backend::TestBackend, Terminal};
use serde_json::json;

fn key(code: KeyCode) -> KeyEvent {
    KeyEvent::new(code, KeyModifiers::NONE)
}
fn save_key() -> KeyEvent {
    KeyEvent::new(KeyCode::Char('s'), KeyModifiers::CONTROL)
}

fn row() -> Provider {
    let mut provider = Provider::with_id(
        "test".into(),
        "测试供应商".into(),
        json!({
            "auth": {"OPENAI_API_KEY": "secret", "extension": "keep"},
            "config": "# preserve comment\nmodel_provider = \"custom\"\nmodel = \"old-model\"\n[model_providers.custom]\nname = \"custom\"\nbase_url = \"https://old.example/v1\"\nwire_api = \"responses\"\n",
            "extra": {"keep": true}
        }),
        None,
    );
    provider.notes = Some("keep notes".into());
    provider
}

fn draft() -> ProviderDraft {
    let original = row();
    ProviderDraft {
        provider: original.clone(),
        base: original.settings_config.clone(),
        original,
        app: AppType::Codex,
        field: 0,
        is_new: false,
    }
}

fn app() -> App {
    let mut app = App::with_apps(vec![AppType::Codex, AppType::Agy], 0);
    app.providers = vec![row()];
    app.current = "test".into();
    app.draft = Some(draft());
    app.page = Page::Providers;
    app.nav_focus = false;
    app
}

#[test]
fn text_editor_handles_unicode_multiline_paste_and_boundaries() {
    let mut editor = TextEditor::new("", true, false);
    editor.insert("中🙂\r\nmodel\t= 1\rnext");
    assert_eq!(editor.text(), "中🙂\nmodel    = 1\nnext");
    editor.key(key(KeyCode::Home));
    editor.key(key(KeyCode::Backspace));
    assert_eq!(editor.text(), "中🙂\nmodel    = 1next");
    editor.key(key(KeyCode::Up));
    editor.key(key(KeyCode::End));
    editor.key(key(KeyCode::Backspace));
    assert_eq!(editor.lines[0], "中");
    editor.key(key(KeyCode::Home));
    editor.key(key(KeyCode::Delete));
    editor.key(key(KeyCode::Delete));
    assert_eq!(editor.text(), "model    = 1next");
    let mut single = TextEditor::new("", false, true);
    single.insert("key\r\nwith\u{1b}control");
    assert_eq!(single.text(), "keywithcontrol");
}

#[test]
fn provider_fields_preserve_metadata_auth_and_toml_comments() {
    let mut draft = draft();
    draft.apply_field(1, "https://new.example/v1").unwrap();
    draft.apply_field(2, "new-key").unwrap();
    draft.apply_field(3, "模型🙂").unwrap();
    assert_eq!(draft.field_text(1), "https://new.example/v1");
    assert_eq!(draft.field_text(2), "new-key");
    assert_eq!(draft.field_text(3), "模型🙂");
    assert!(draft.field_text(5).contains("# preserve comment"));
    assert_eq!(draft.provider.settings_config["auth"]["extension"], "keep");
    assert_eq!(draft.provider.settings_config["extra"]["keep"], true);
    assert_eq!(draft.provider.notes.as_deref(), Some("keep notes"));
    let original = draft.provider.settings_config.clone();
    for invalid in [
        "{",
        "[]",
        "null",
        r#"{"auth":null}"#,
        r#"{"auth":{},"config":"[invalid"}"#,
    ] {
        assert!(draft.apply_field(4, invalid).is_err());
        assert_eq!(draft.provider.settings_config, original);
    }
    assert!(draft.apply_field(5, "model = [").is_err());
    assert_eq!(draft.provider.settings_config, original);
    assert!(draft.apply_field(0, " ").is_err());
}

#[test]
fn agy_model_clear_removes_legacy_fallback_and_preserves_other_settings() {
    let mut draft = draft();
    draft.app = AppType::Agy;
    draft.provider.settings_config =
        json!({"env":{"GEMINI_API_KEY":"k","GEMINI_MODEL":"old"}, "config":{"theme":"dark"}});
    draft.apply_field(3, "new").unwrap();
    assert_eq!(draft.field_text(3), "new");
    draft.apply_field(3, "").unwrap();
    assert_eq!(draft.field_text(3), "");
    assert_eq!(draft.provider.settings_config["config"]["theme"], "dark");
    assert_eq!(draft.provider.settings_config["env"]["GEMINI_API_KEY"], "k");
}

#[test]
fn cli_settings_merge_other_fields_and_refuse_corruption() {
    let temp = tempfile::tempdir().unwrap();
    let path = temp.path().join("cli.json");
    std::fs::write(&path, r#"{"usage_recording":false,"extension":{"keep":1}}"#).unwrap();
    let mut draft = SettingsDraft::load(&path).unwrap();
    draft.values[0] = true;
    // A concurrent change to another switch must survive this save.
    std::fs::write(
        &path,
        r#"{"usage_recording":false,"proxy_log":true,"extension":{"keep":2}}"#,
    )
    .unwrap();
    draft.save(&path).unwrap();
    let saved = super::super::read_json_object_or_default(&path).unwrap();
    assert_eq!(
        saved,
        json!({"usage_recording":true,"proxy_log":true,"extension":{"keep":2}})
    );
    let mut draft = SettingsDraft::load(&path).unwrap();
    draft.values[0] = false;
    for broken in ["{", "[]", r#"{"usage_recording":"invalid"}"#] {
        std::fs::write(&path, broken).unwrap();
        assert!(draft.save(&path).is_err());
        assert!(SettingsDraft::load(&path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), broken);
    }
}

#[test]
fn cancel_requires_confirmation_and_invalid_editor_stays_open() {
    let state = AppState::new(Arc::new(Database::memory().unwrap()));
    let mut app = app();
    app.key(key(KeyCode::Enter), &state).unwrap();
    app.editor.as_mut().unwrap().insert("新");
    app.key(key(KeyCode::Esc), &state).unwrap();
    assert!(app.confirm.is_some());
    app.key(key(KeyCode::Char('n')), &state).unwrap();
    assert!(app.editor.is_some());
    app.key(key(KeyCode::Esc), &state).unwrap();
    app.key(key(KeyCode::Char('y')), &state).unwrap();
    assert!(!app.draft.as_ref().unwrap().dirty());
    app.draft.as_mut().unwrap().field = 4;
    app.key(key(KeyCode::Enter), &state).unwrap();
    app.editor = Some(TextEditor::new("{invalid", true, false));
    assert!(app.key(save_key(), &state).is_err());
    assert_eq!(app.editor.as_ref().unwrap().text(), "{invalid");
    assert!(!app.draft.as_ref().unwrap().dirty());
    app.key(
        KeyEvent::new(KeyCode::Char('c'), KeyModifiers::CONTROL),
        &state,
    )
    .unwrap();
    assert!(!app.quit);
    app.key(key(KeyCode::Char('y')), &state).unwrap();
    assert!(app.quit);
}

#[test]
fn terminal_render_masks_keys_and_handles_small_sizes() {
    let mut app = app();
    for (width, height) in [(100, 30), (48, 12), (10, 3)] {
        let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        let output: String = terminal
            .backend()
            .buffer()
            .content
            .iter()
            .map(|cell| cell.symbol())
            .collect();
        assert!(!output.contains("secret"));
    }
    app.draft.as_mut().unwrap().field = 2;
    app.editor = Some(TextEditor::new("secret", false, true));
    let mut terminal = Terminal::new(TestBackend::new(80, 24)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let output: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|cell| cell.symbol())
        .collect();
    assert!(output.contains("******"));
    assert!(!output.contains("secret"));
}

#[test]
fn editor_cursor_matches_rendered_control_characters() {
    let mut app = app();
    app.draft.as_mut().unwrap().field = 5;
    for text in ["a\tb".to_owned(), format!("{}\tb", "中".repeat(60))] {
        let mut editor = TextEditor::new(&text, true, false);
        editor.key(key(KeyCode::End));
        app.editor = Some(editor);
        for (width, height) in [(100, 30), (48, 12)] {
            let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
            terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
            let cursor = terminal.get_cursor_position().unwrap();
            assert_eq!(
                terminal.backend().buffer()[(cursor.x - 1, cursor.y)].symbol(),
                "b",
                "the cursor must follow the final visible character"
            );
        }
    }
}

#[test]
fn provider_save_roundtrip_uses_core_editor_and_preserves_live_changes() {
    // This is the only unit test that changes process-wide configuration. Restore it even on panic.
    struct Restore(Option<std::ffi::OsString>);
    impl Drop for Restore {
        fn drop(&mut self) {
            config::clear_app_config_dir_override();
            match &self.0 {
                Some(value) => std::env::set_var("CC_SWITCH_TEST_HOME", value),
                None => std::env::remove_var("CC_SWITCH_TEST_HOME"),
            }
            let _ = cc_switch_core::settings::reload_settings();
        }
    }
    let home = tempfile::tempdir().unwrap();
    let _restore = Restore(std::env::var_os("CC_SWITCH_TEST_HOME"));
    std::env::set_var("CC_SWITCH_TEST_HOME", home.path());
    config::set_app_config_dir_override(home.path().join(".cc-switch"));
    config::write_json_file_private(&home.path().join(".cc-switch/settings.json"), &json!({}))
        .unwrap();
    cc_switch_core::settings::reload_settings().unwrap();
    let state = AppState::new(Arc::new(Database::memory().unwrap()));
    let provider = row();
    state.db.save_provider(&AppType::Codex, &provider).unwrap();
    let live_path = cc_switch_core::codex_config::get_codex_config_path();
    std::fs::create_dir_all(live_path.parent().unwrap()).unwrap();
    std::fs::write(
        &live_path,
        "# live comment\nmodel = \"native\"\napproval_policy = \"on-request\"\n",
    )
    .unwrap();
    let mut draft = ProviderDraft::open(&state, AppType::Codex, &provider.id).unwrap();
    draft.apply_field(0, "新名称").unwrap();
    draft.apply_field(3, "new-model").unwrap();
    // Unedited live fields may change while the draft is open.
    std::fs::write(
        &live_path,
        "# live comment\nmodel = \"native\"\napproval_policy = \"never\"\n",
    )
    .unwrap();
    draft.save(&state).unwrap();
    let saved = state
        .db
        .get_provider_by_id(&provider.id, &AppType::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(saved.name, "新名称");
    assert_eq!(saved.notes, provider.notes);
    assert!(saved.settings_config["config"]
        .as_str()
        .unwrap()
        .contains("new-model"));
    assert!(std::fs::read_to_string(&live_path)
        .unwrap()
        .contains("approval_policy = \"never\""));
    assert!(draft.save(&state).unwrap_err().contains("其他窗口修改"));
    let mut draft = ProviderDraft::open(&state, AppType::Codex, &provider.id).unwrap();
    // Global TOML conflicts must not overwrite newer native config or the saved row.
    let text = draft.field_text(5).replace("\"never\"", "\"on-request\"");
    draft.apply_field(5, &text).unwrap();
    std::fs::write(
        &live_path,
        "model = \"native\"\napproval_policy = \"untrusted\"\n",
    )
    .unwrap();
    assert!(draft.save(&state).is_err());
    assert!(std::fs::read_to_string(&live_path)
        .unwrap()
        .contains("untrusted"));
    let unchanged = state
        .db
        .get_provider_by_id(&provider.id, &AppType::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(
        serde_json::to_value(unchanged).unwrap(),
        serde_json::to_value(saved).unwrap()
    );

    // Editing the active provider must update the native route, including its bearer key.
    ProviderService::switch(&state, AppType::Codex, &provider.id).unwrap();
    let mut draft = ProviderDraft::open(&state, AppType::Codex, &provider.id).unwrap();
    draft.apply_field(1, "https://updated.example/v1").unwrap();
    draft.apply_field(2, "updated-key").unwrap();
    draft.apply_field(3, "updated-model").unwrap();
    draft.save(&state).unwrap();
    let live = std::fs::read_to_string(&live_path).unwrap();
    assert!(live.contains("https://updated.example/v1"));
    assert!(live.contains("updated-model"));
    assert!(live.contains("untrusted"));
    let live_doc: toml_edit::DocumentMut = live.parse().unwrap();
    let route = live_doc["model_provider"].as_str().unwrap();
    assert_eq!(
        live_doc["model_providers"][route]["experimental_bearer_token"].as_str(),
        Some("updated-key")
    );
    assert!(!live_path.with_file_name("auth.json").exists());

    // Antigravity is a launch card: editing it does not create a native settings file.
    let agy = Provider::with_id(
        "agy-test".into(),
        "Agy".into(),
        json!({"env":{},"config":null}),
        None,
    );
    state.db.save_provider(&AppType::Agy, &agy).unwrap();
    let mut draft = ProviderDraft::open(&state, AppType::Agy, &agy.id).unwrap();
    draft.apply_field(1, "https://agy.example").unwrap();
    draft.apply_field(2, "agy-key").unwrap();
    draft.apply_field(3, "agy-model").unwrap();
    draft.save(&state).unwrap();
    let saved = state
        .db
        .get_provider_by_id(&agy.id, &AppType::Agy)
        .unwrap()
        .unwrap();
    assert_eq!(
        saved.settings_config["config"]["model"]["name"],
        "agy-model"
    );
    assert_eq!(saved.settings_config["env"]["GEMINI_API_KEY"], "agy-key");
    assert!(!cc_switch_core::agy_config::settings_path().exists());

    exercise_management_forms(&state);
}

fn exercise_management_forms(state: &AppState) {
    use cc_switch_core::services::{McpService, PromptService, SkillService};
    // Exercise the new flows under the same isolated home as the provider tests.
    let mut provider = ProviderDraft::create(state, AppType::Codex, None).unwrap();
    assert!(provider.save(state).unwrap_err().contains("名称"));
    provider.apply_field(0, "新增供应商").unwrap();
    provider.apply_field(1, "https://new.example/v1").unwrap();
    provider.apply_field(2, "new-provider-key").unwrap();
    provider.apply_field(3, "new-model").unwrap();
    provider.save(state).unwrap();
    let saved = state
        .db
        .get_provider_by_id(&provider.provider.id, &AppType::Codex)
        .unwrap()
        .unwrap();
    assert_eq!(saved.name, "新增供应商");
    assert!(
        provider.save(state).is_err(),
        "new IDs must not overwrite existing providers"
    );
    let copy = ProviderDraft::create(state, AppType::Codex, Some(&saved)).unwrap();
    assert_ne!(copy.provider.id, saved.id);
    copy.save(state).unwrap();
    assert_eq!(
        ProviderService::current(state, AppType::Codex).unwrap(),
        "test"
    );
    actions::execute(Action::DeleteProvider(saved), state, &AppType::Codex).unwrap();

    let mut mcp = Form::mcp(None, AppType::Codex);
    mcp.fields[1].value = "本地 MCP".into();
    mcp.fields[3].value =
        r#"{"command":"echo","args":["test"],"env":{"TOKEN":"mcp-secret"}}"#.into();
    mcp.save(state).unwrap();
    let id = mcp.fields[0].value.clone();
    let server = McpService::get_all_servers(state).unwrap()[&id].clone();
    assert!(!server.apps.codex);
    actions::execute(Action::ToggleMcp(server), state, &AppType::Codex).unwrap();
    let server = McpService::get_all_servers(state).unwrap()[&id].clone();
    assert!(server.apps.codex);
    let mut edit = Form::mcp(Some(server.clone()), AppType::Codex);
    edit.fields[3].value = "{broken".into();
    assert!(edit.save(state).is_err());
    assert_eq!(
        McpService::get_all_servers(state).unwrap()[&id].server,
        server.server
    );
    let mut edit = Form::mcp(Some(server.clone()), AppType::Codex);
    edit.fields[1].value = "新名称".into();
    edit.save(state).unwrap();
    assert!(actions::execute(Action::DeleteMcp(server), state, &AppType::Codex).is_err());
    let server = McpService::get_all_servers(state).unwrap()[&id].clone();
    actions::execute(Action::DeleteMcp(server), state, &AppType::Codex).unwrap();
    assert!(!McpService::get_all_servers(state)
        .unwrap()
        .contains_key(&id));
    let native =
        std::fs::read_to_string(cc_switch_core::codex_config::get_codex_config_path()).unwrap();
    assert!(!native.contains(&id));
    assert!(native.contains("updated-model"));

    let mut prompt = Form::prompt(None, AppType::Codex).unwrap();
    prompt.fields[0].value = "代码审查".into();
    prompt.fields[2].value = "# Review\n检查边界情况。".into();
    prompt.save(state).unwrap();
    let value = PromptService::get_prompts(state, AppType::Codex)
        .unwrap()
        .into_values()
        .find(|p| p.name == "代码审查")
        .unwrap();
    actions::execute(Action::TogglePrompt(value), state, &AppType::Codex).unwrap();
    assert_eq!(
        PromptService::get_current_file_content(AppType::Codex)
            .unwrap()
            .unwrap(),
        "# Review\n检查边界情况。"
    );
    let value = PromptService::get_prompts(state, AppType::Codex)
        .unwrap()
        .into_values()
        .find(|p| p.enabled)
        .unwrap();
    assert!(actions::execute(Action::DeletePrompt(value.clone()), state, &AppType::Codex).is_err());
    // The list may predate a native-file edit. Opening a form and changing only
    // its description must not restore the stale list's prompt content.
    let live = cc_switch_core::prompt_files::prompt_file_path(&AppType::Codex).unwrap();
    std::fs::write(&live, "External edit before opening").unwrap();
    let mut edit = Form::prompt(Some(value), AppType::Codex).unwrap();
    assert_eq!(edit.fields[2].value, "External edit before opening");
    edit.fields[1].value = "Updated description".into();
    edit.save(state).unwrap();
    assert_eq!(
        std::fs::read_to_string(&live).unwrap(),
        "External edit before opening"
    );
    let value = PromptService::get_prompts(state, AppType::Codex)
        .unwrap()
        .into_values()
        .find(|p| p.enabled)
        .unwrap();
    assert_eq!(value.content, "External edit before opening");
    let mut edit = Form::prompt(Some(value.clone()), AppType::Codex).unwrap();
    edit.fields[2].value = "My edit".into();
    std::fs::write(&live, "External edit").unwrap();
    assert!(
        edit.save(state).is_err(),
        "concurrent native prompt changes must survive"
    );
    assert_eq!(std::fs::read_to_string(&live).unwrap(), "External edit");
    assert!(actions::execute(Action::TogglePrompt(value), state, &AppType::Codex).is_err());
    assert_eq!(std::fs::read_to_string(&live).unwrap(), "External edit");
    let value = PromptService::get_prompts(state, AppType::Codex)
        .unwrap()
        .into_values()
        .find(|p| p.enabled)
        .unwrap();
    actions::execute(Action::TogglePrompt(value), state, &AppType::Codex).unwrap();
    let value = PromptService::get_prompts(state, AppType::Codex)
        .unwrap()
        .into_values()
        .find(|p| p.name == "代码审查")
        .unwrap();
    actions::execute(Action::DeletePrompt(value), state, &AppType::Codex).unwrap();

    let mut repo = Form::repo(None);
    repo.fields[0].value = "example".into();
    repo.fields[1].value = "test-skills".into();
    repo.save(state).unwrap();
    assert!(repo.save(state).is_err());
    let saved_repo = state
        .db
        .get_skill_repos()
        .unwrap()
        .into_iter()
        .find(|r| r.owner == "example")
        .unwrap();
    actions::execute(Action::DeleteRepo(saved_repo), state, &AppType::Codex).unwrap();

    let dir = SkillService::get_installed_skill_dir("test-tui-skill").unwrap();
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(
        dir.join("SKILL.md"),
        "---\nname: Test\ndescription: Local test\n---\nTest skill",
    )
    .unwrap();
    let skill = cc_switch_core::app_config::InstalledSkill {
        id: "local:test-tui-skill".into(),
        name: "Test".into(),
        description: None,
        directory: "test-tui-skill".into(),
        repo_owner: None,
        repo_name: None,
        repo_branch: None,
        readme_url: None,
        apps: Default::default(),
        installed_at: 1,
        content_hash: None,
        updated_at: 0,
    };
    state.db.save_skill(&skill).unwrap();
    actions::execute(Action::ToggleSkill(skill.clone()), state, &AppType::Codex).unwrap();
    let native = SkillService::get_app_skills_dir(&AppType::Codex)
        .unwrap()
        .join("test-tui-skill/SKILL.md");
    assert!(native.exists());
    let viewed =
        actions::execute(Action::ReadSkill(skill.clone()), state, &AppType::Codex).unwrap();
    assert!(viewed.view.unwrap().1.contains("Test skill"));
    actions::execute(Action::DeleteSkill(skill), state, &AppType::Codex).unwrap();
    assert!(!native.exists());
    assert!(state
        .db
        .get_installed_skill("local:test-tui-skill")
        .unwrap()
        .is_none());
}

#[test]
fn navigation_filter_and_confirmation_target_visible_item() {
    let state = AppState::new(Arc::new(Database::memory().unwrap()));
    let mut app = app();
    app.draft = None;
    let mut second = row();
    second.id = "second".into();
    second.name = "第二家".into();
    app.providers.push(second);
    app.key(key(KeyCode::Char('/')), &state).unwrap();
    for c in "第二".chars() {
        app.key(key(KeyCode::Char(c)), &state).unwrap();
    }
    app.key(key(KeyCode::Enter), &state).unwrap();
    assert_eq!(app.visible(), vec![1]);
    app.key(key(KeyCode::Char('d')), &state).unwrap();
    assert!(
        matches!(&app.confirm, Some(Confirm::Action(_, Action::DeleteProvider(p))) if p.id == "second")
    );
    app.key(key(KeyCode::Char('n')), &state).unwrap();
    assert!(app.pending.is_none());
    app.key(key(KeyCode::Enter), &state).unwrap();
    app.key(key(KeyCode::Char('y')), &state).unwrap();
    assert!(
        matches!(&app.pending, Some(Action::Cli(args)) if args == &vec!["provider", "switch", "second"])
    );
    app.pending = None;
    app.key(key(KeyCode::Esc), &state).unwrap();
    assert!(app.filter.is_empty());
    app.key(key(KeyCode::Char('3')), &state).unwrap();
    assert_eq!(app.page, Page::Mcp);
    app.pending = None;
    app.key(key(KeyCode::Char('a')), &state).unwrap();
    assert!(app.form.is_some());
    app.form.as_mut().unwrap().selected = 1;
    app.key(key(KeyCode::Enter), &state).unwrap();
    app.editor.as_mut().unwrap().insert("New");
    app.key(key(KeyCode::Enter), &state).unwrap();
    app.key(key(KeyCode::Esc), &state).unwrap();
    assert!(matches!(app.confirm, Some(Confirm::Discard(Discard::Form))));
    app.key(key(KeyCode::Char('n')), &state).unwrap();
    assert_eq!(app.form.as_ref().unwrap().fields[1].value, "New");
    app.status = "validation details".into();
    app.key(key(KeyCode::F(4)), &state).unwrap();
    assert_eq!(app.viewer.as_ref().unwrap().text, "validation details");
    app.key(key(KeyCode::Esc), &state).unwrap();
    app.busy = Some("保存".into());
    app.key(key(KeyCode::Char('q')), &state).unwrap();
    assert!(!app.quit);
    app.key(key(KeyCode::Char('?')), &state).unwrap();
    assert!(app.help);
}

#[test]
fn all_pages_render_at_terminal_sizes_without_exposing_credentials() {
    let mut app = app();
    app.draft = None;
    for page in Page::NAV.into_iter().chain([Page::Discover, Page::Repos]) {
        app.page = page;
        for (width, height) in [(120, 36), (88, 24), (80, 24), (48, 12), (10, 3)] {
            for nav in [true, false] {
                app.nav_focus = nav;
                let mut terminal = Terminal::new(TestBackend::new(width, height)).unwrap();
                terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
                let output: String = terminal
                    .backend()
                    .buffer()
                    .content
                    .iter()
                    .map(|c| c.symbol())
                    .collect();
                assert!(!output.contains("secret"));
                if width >= 48 {
                    assert!(output.contains("CC-SWITCH"));
                }
            }
        }
    }
    app.page = Page::Mcp;
    app.form = Some(Form::mcp(None, AppType::Codex));
    app.form.as_mut().unwrap().fields[3].value = r#"{"env":{"TOKEN":"secret"}}"#.into();
    let mut terminal = Terminal::new(TestBackend::new(100, 26)).unwrap();
    terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
    let output: String = terminal
        .backend()
        .buffer()
        .content
        .iter()
        .map(|c| c.symbol())
        .collect();
    assert!(!output.contains("secret"));
}

#[test]
fn export_tui_preview_when_requested() {
    let Some(output) = std::env::var_os("CC_SWITCH_TUI_RENDER_DIR") else {
        return;
    };
    let output = std::path::PathBuf::from(output);
    std::fs::create_dir_all(&output).unwrap();
    let mut app = app();
    app.draft = None;
    app.providers[0].name = "主力供应商".into();
    for (id, name) in [
        ("backup", "备用供应商"),
        ("local", "本地开发"),
        ("official", "官方账号"),
    ] {
        let mut p = row();
        p.id = id.into();
        p.name = name.into();
        app.providers.push(p);
    }
    app.data.proxy = "监听地址       127.0.0.1:15721\n后台状态       未运行\n用量记录       关闭\n代理日志       关闭".into();
    for (name, page) in [
        ("home", Page::Home),
        ("providers", Page::Providers),
        ("mcp", Page::Mcp),
    ] {
        app.page = page;
        app.nav_index = page.nav_index();
        app.nav_focus = false;
        if page == Page::Mcp {
            app.form = Some(Form::mcp(None, AppType::Codex));
            app.form.as_mut().unwrap().fields[1].value = "Filesystem".into();
        }
        let mut terminal = Terminal::new(TestBackend::new(120, 32)).unwrap();
        terminal.draw(|frame| ui::draw(frame, &app)).unwrap();
        let cells: Vec<_> = terminal.backend().buffer().content.iter().map(|c| json!({"text": c.symbol(), "fg": c.fg.to_string(), "bg": c.bg.to_string(), "bold": c.modifier.contains(ratatui::style::Modifier::BOLD)})).collect();
        std::fs::write(
            output.join(format!("{name}.json")),
            serde_json::to_vec(&json!({"width":120,"height":32,"cells":cells})).unwrap(),
        )
        .unwrap();
    }
}
