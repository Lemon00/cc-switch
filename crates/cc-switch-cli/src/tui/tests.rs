use super::*;
use cc_switch_core::{config, Database};
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
    }
}

fn app() -> App {
    App {
        apps: vec![AppType::Codex, AppType::Agy],
        app_index: 0,
        providers: vec![row()],
        selected: 0,
        current: "test".into(),
        settings_page: false,
        settings: None,
        draft: Some(draft()),
        editor: None,
        confirm: None,
        help: false,
        status: String::new(),
        quit: false,
    }
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
}
