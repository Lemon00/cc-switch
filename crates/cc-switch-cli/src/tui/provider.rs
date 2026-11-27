use cc_switch_core::provider::Provider;
use cc_switch_core::services::provider::EditorSave;
use cc_switch_core::{AppState, AppType, ProviderService};
use serde_json::{json, Value};
use toml_edit::{value, DocumentMut, Item};

pub(super) const FIELDS: &[&str] = &[
    "名称",
    "API 地址",
    "API Key",
    "模型",
    "原始配置 JSON（含凭据）",
    "Codex config.toml",
];

pub(super) struct ProviderDraft {
    pub original: Provider,
    pub provider: Provider,
    pub base: Value,
    pub app: AppType,
    pub field: usize,
}

impl ProviderDraft {
    pub fn open(state: &AppState, app: AppType, id: &str) -> Result<Self, String> {
        let original = state
            .db
            .get_provider_by_id(id, &app)
            .map_err(|e| e.to_string())?
            .ok_or("供应商已被删除，请刷新列表")?;
        let base = if app == AppType::Codex {
            let category =
                ProviderService::editor_category(state, &app, Some(id), original.category.clone())
                    .map_err(|e| e.to_string())?;
            ProviderService::editor_view_with_meta(
                state,
                app.clone(),
                &original.settings_config,
                category.as_deref(),
                Some(id),
                original.meta.as_ref(),
            )
            .map_err(|e| e.to_string())?
            .settings
        } else {
            original.settings_config.clone()
        };
        validate_settings(&app, &base)?;
        let mut provider = original.clone();
        provider.settings_config = base.clone();
        Ok(Self {
            original,
            provider,
            base,
            app,
            field: 0,
        })
    }

    pub fn field_count(&self) -> usize {
        if self.app == AppType::Codex {
            FIELDS.len()
        } else {
            FIELDS.len() - 1
        }
    }

    pub fn dirty(&self) -> bool {
        self.provider.name != self.original.name || self.provider.settings_config != self.base
    }

    pub fn field_text(&self, field: usize) -> String {
        let settings = &self.provider.settings_config;
        match field {
            0 => self.provider.name.clone(),
            1..=3 if self.app == AppType::Codex => {
                if field == 2 {
                    return settings
                        .pointer("/auth/OPENAI_API_KEY")
                        .and_then(Value::as_str)
                        .unwrap_or_default()
                        .into();
                }
                let doc = config_doc(settings).expect("validated draft TOML");
                if field == 3 {
                    doc.get("model")
                        .and_then(Item::as_str)
                        .unwrap_or_default()
                        .into()
                } else {
                    let route = doc
                        .get("model_provider")
                        .and_then(Item::as_str)
                        .unwrap_or("openai");
                    doc.get("model_providers")
                        .and_then(|p| p.get(route))
                        .and_then(|p| p.get("base_url"))
                        .and_then(Item::as_str)
                        .unwrap_or_default()
                        .into()
                }
            }
            1 => settings
                .pointer("/env/GOOGLE_GEMINI_BASE_URL")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            2 => settings
                .pointer("/env/GEMINI_API_KEY")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            3 => settings
                .pointer("/config/model/name")
                .or_else(|| settings.pointer("/env/GEMINI_MODEL"))
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            4 => serde_json::to_string_pretty(settings).unwrap_or_default(),
            5 => settings
                .get("config")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            _ => String::new(),
        }
    }

    pub fn apply_field(&mut self, field: usize, text: &str) -> Result<(), String> {
        // Do not rewrite untouched values (in particular native/managed auth).
        if text == self.field_text(field) {
            return Ok(());
        }
        if field == 0 {
            if text.trim().is_empty() {
                return Err("名称不能为空".into());
            }
            self.provider.name = text.trim().into();
            return Ok(());
        }
        let mut next = self.provider.settings_config.clone();
        match field {
            4 => {
                next = serde_json::from_str(text).map_err(|e| {
                    format!("JSON 格式错误：第 {} 行，第 {} 列", e.line(), e.column())
                })?
            }
            5 => {
                next["config"] = json!(text);
            }
            1..=3 if self.app == AppType::Codex => {
                if field == 2 {
                    set_json_text(&mut next, &["auth", "OPENAI_API_KEY"], text)?;
                } else {
                    let mut doc = config_doc(&next)?;
                    if field == 3 {
                        if text.is_empty() {
                            doc.remove("model");
                        } else {
                            doc["model"] = value(text);
                        }
                    } else {
                        let route = doc
                            .get("model_provider")
                            .and_then(Item::as_str)
                            .ok_or("此配置未选择自定义供应商，请先在 TOML 中设置 model_provider")?
                            .to_owned();
                        let table = doc
                            .get_mut("model_providers")
                            .and_then(|p| p.get_mut(&route))
                            .and_then(Item::as_table_like_mut)
                            .ok_or("找不到所选供应商的 TOML 表，请在原始 TOML 中编辑地址")?;
                        if text.is_empty() {
                            table.remove("base_url");
                        } else {
                            table.insert("base_url", value(text));
                        }
                    }
                    next["config"] = json!(doc.to_string());
                }
            }
            1 => set_json_text(&mut next, &["env", "GOOGLE_GEMINI_BASE_URL"], text)?,
            2 => set_json_text(&mut next, &["env", "GEMINI_API_KEY"], text)?,
            3 => {
                set_json_text(&mut next, &["config", "model", "name"], text)?;
                // Clearing a model must also clear the legacy fallback.
                if let Some(env) = next.get_mut("env").and_then(Value::as_object_mut) {
                    env.remove("GEMINI_MODEL");
                }
            }
            _ => return Err("未知字段".into()),
        }
        validate_settings(&self.app, &next)?;
        self.provider.settings_config = next;
        Ok(())
    }

    pub fn save(&self, state: &AppState) -> Result<(), String> {
        if !self.dirty() {
            return Ok(());
        }
        let _guard =
            super::super::lock_proxy_control(&super::super::app_dir().join("cli-control.lock"))?;
        let current = state
            .db
            .get_provider_by_id(&self.original.id, &self.app)
            .map_err(|e| e.to_string())?
            .ok_or("供应商已被删除，无法保存")?;
        if serde_json::to_value(&current).unwrap() != serde_json::to_value(&self.original).unwrap()
        {
            return Err("供应商已被其他窗口修改；请取消草稿并重新打开后编辑".into());
        }
        validate_settings(&self.app, &self.provider.settings_config)?;
        let editor = (self.app == AppType::Codex).then(|| EditorSave {
            base: self.base.clone(),
            draft: None,
            on_conflict: Default::default(),
        });
        ProviderService::update_from_editor(
            state,
            self.app.clone(),
            None,
            self.provider.clone(),
            editor,
        )
        .map_err(|e| e.to_string())?;
        Ok(())
    }
}

fn config_doc(settings: &Value) -> Result<DocumentMut, String> {
    settings
        .get("config")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .parse::<DocumentMut>()
        .map_err(|_| "TOML 格式错误，请修正后再保存".into())
}

fn validate_settings(app: &AppType, settings: &Value) -> Result<(), String> {
    if !settings.is_object() {
        return Err("配置必须是 JSON 对象".into());
    }
    if *app == AppType::Codex {
        if !settings.get("auth").is_some_and(Value::is_object) {
            return Err("Codex auth 必须是 JSON 对象".into());
        }
        if settings
            .get("config")
            .is_some_and(|v| !v.is_string() && !v.is_null())
        {
            return Err("Codex config 必须是 TOML 字符串".into());
        }
        config_doc(settings)?;
    } else {
        if settings.get("env").is_some_and(|v| !v.is_object()) {
            return Err("env 必须是 JSON 对象".into());
        }
        for path in ["/config", "/config/model"] {
            if settings
                .pointer(path)
                .is_some_and(|v| !v.is_object() && !v.is_null())
            {
                return Err(format!("{path} 必须是 JSON 对象"));
            }
        }
        for path in [
            "/env/GEMINI_API_KEY",
            "/env/GOOGLE_GEMINI_BASE_URL",
            "/env/GEMINI_MODEL",
            "/config/model/name",
        ] {
            if settings
                .pointer(path)
                .is_some_and(|v| !v.is_string() && !v.is_null())
            {
                return Err(format!("{path} 必须是字符串"));
            }
        }
    }
    Ok(())
}

fn set_json_text(root: &mut Value, path: &[&str], text: &str) -> Result<(), String> {
    let mut node = root;
    for key in &path[..path.len() - 1] {
        if node.is_null() {
            *node = json!({});
        }
        let object = node
            .as_object_mut()
            .ok_or("字段父节点必须是 JSON 对象，请编辑原始 JSON")?;
        node = object.entry((*key).to_owned()).or_insert_with(|| json!({}));
    }
    if node.is_null() {
        *node = json!({});
    }
    let object = node
        .as_object_mut()
        .ok_or("字段父节点必须是 JSON 对象，请编辑原始 JSON")?;
    let key = path[path.len() - 1];
    if text.is_empty() {
        object.remove(key);
    } else {
        object.insert(key.into(), json!(text));
    }
    Ok(())
}
