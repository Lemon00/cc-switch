use cc_switch_core::app_config::{McpApps, McpServer};
use cc_switch_core::prompt::{Prompt, PromptWriteExpectation};
use cc_switch_core::services::{PromptService, SkillRepo, SkillService};
use cc_switch_core::{AppState, AppType};
use serde_json::json;

use super::data::new_id;

#[derive(Clone)]
pub(super) struct Field {
    pub label: String,
    pub value: String,
    pub multiline: bool,
    pub toggle: bool,
    pub readonly: bool,
}

impl Field {
    fn text(label: &str, value: impl Into<String>) -> Self {
        Self {
            label: label.into(),
            value: value.into(),
            multiline: false,
            toggle: false,
            readonly: false,
        }
    }

    fn multiline(label: &str, value: impl Into<String>) -> Self {
        Self {
            multiline: true,
            ..Self::text(label, value)
        }
    }

    fn toggle(label: &str, enabled: bool) -> Self {
        Self {
            toggle: true,
            ..Self::text(label, enabled.to_string())
        }
    }
}

#[derive(Clone)]
pub(super) enum Kind {
    Mcp {
        original: Option<McpServer>,
        app: AppType,
    },
    Prompt {
        expected: PromptWriteExpectation,
        id: String,
        app: AppType,
    },
    Repo {
        original: Option<SkillRepo>,
    },
    Zip,
}

#[derive(Clone)]
pub(super) struct Form {
    pub title: String,
    pub fields: Vec<Field>,
    pub selected: usize,
    pub kind: Kind,
    base: Vec<String>,
}

impl Form {
    fn new(title: &str, fields: Vec<Field>, kind: Kind) -> Self {
        let base = fields.iter().map(|f| f.value.clone()).collect();
        Self {
            title: title.into(),
            fields,
            kind,
            base,
            selected: 0,
        }
    }

    pub fn mcp(original: Option<McpServer>, app: AppType) -> Self {
        let server = original.clone().unwrap_or_else(|| McpServer {
            id: new_id(), name: String::new(), server: json!({"command": "npx", "args": ["-y", "@modelcontextprotocol/server-filesystem", "/path/to/folder"]}),
            apps: McpApps::default(), description: None, homepage: None, docs: None, tags: vec![],
        });
        let mut id = Field::text("ID", &server.id);
        id.readonly = original.is_some();
        Self::new(
            if original.is_some() {
                "编辑 MCP 服务器"
            } else {
                "新增 MCP 服务器"
            },
            vec![
                id,
                Field::text("名称", server.name),
                Field::text("描述", server.description.unwrap_or_default()),
                Field::multiline(
                    "连接配置 JSON（含环境变量 / 凭据）",
                    serde_json::to_string_pretty(&server.server).unwrap_or_default(),
                ),
                Field::toggle(
                    &format!("在 {} 启用", app.as_str()),
                    server.apps.is_enabled_for(&app),
                ),
            ],
            Kind::Mcp { original, app },
        )
    }

    pub fn prompt(original: Option<Prompt>, app: AppType) -> Result<Self, String> {
        let file_content =
            PromptService::get_current_file_content(app.clone()).map_err(|e| e.to_string())?;
        let id = original
            .as_ref()
            .map(|p| p.id.clone())
            .unwrap_or_else(new_id);
        // The list can be older than the native file. Show the same live content
        // captured by the save expectation, keeping the DB snapshot for conflicts.
        let content = original.as_ref().map_or("", |prompt| {
            if prompt.enabled {
                file_content.as_deref().unwrap_or(&prompt.content)
            } else {
                &prompt.content
            }
        });
        let fields = vec![
            Field::text(
                "名称",
                original
                    .as_ref()
                    .map(|p| p.name.as_str())
                    .unwrap_or_default(),
            ),
            Field::text(
                "描述",
                original
                    .as_ref()
                    .and_then(|p| p.description.as_deref())
                    .unwrap_or_default(),
            ),
            Field::multiline("提示词 Markdown", content),
        ];
        Ok(Self::new(
            if original.is_some() {
                "编辑提示词"
            } else {
                "新增提示词"
            },
            fields,
            Kind::Prompt {
                expected: PromptWriteExpectation {
                    prompt: original,
                    file_content,
                },
                id,
                app,
            },
        ))
    }

    pub fn repo(original: Option<SkillRepo>) -> Self {
        let mut fields = vec![
            Field::text(
                "GitHub 用户 / 组织",
                original
                    .as_ref()
                    .map(|r| r.owner.as_str())
                    .unwrap_or_default(),
            ),
            Field::text(
                "仓库名称",
                original
                    .as_ref()
                    .map(|r| r.name.as_str())
                    .unwrap_or_default(),
            ),
            Field::text(
                "分支",
                original
                    .as_ref()
                    .map(|r| r.branch.as_str())
                    .unwrap_or("main"),
            ),
            Field::toggle("启用发现", original.as_ref().is_none_or(|r| r.enabled)),
        ];
        fields[0].readonly = original.is_some();
        fields[1].readonly = original.is_some();
        Self::new("技能仓库", fields, Kind::Repo { original })
    }

    pub fn zip() -> Self {
        Self::new(
            "从本地 ZIP 安装技能",
            vec![Field::text("ZIP 文件路径", "")],
            Kind::Zip,
        )
    }

    pub fn dirty(&self) -> bool {
        self.fields
            .iter()
            .zip(&self.base)
            .any(|(f, base)| f.value != *base)
    }

    pub fn apply(&mut self, value: String) {
        if !self.fields[self.selected].readonly {
            self.fields[self.selected].value = value;
        }
    }

    pub fn mcp_value(&self) -> Result<McpServer, String> {
        let Kind::Mcp { original, app } = &self.kind else {
            return Err("不是 MCP 表单".into());
        };
        let id = self.fields[0].value.trim();
        let name = self.fields[1].value.trim();
        if id.is_empty() || name.is_empty() || id.chars().any(char::is_control) {
            return Err("ID 和名称不能为空，ID 不能含控制字符".into());
        }
        let spec = parse_json(&self.fields[3].value)?;
        cc_switch_core::mcp::validation::validate_server_spec(&spec).map_err(|e| e.to_string())?;
        let mut server = original.clone().unwrap_or_else(|| McpServer {
            id: id.into(),
            name: name.into(),
            server: json!({}),
            apps: McpApps::default(),
            description: None,
            homepage: None,
            docs: None,
            tags: vec![],
        });
        server.name = name.into();
        server.description = nonempty(&self.fields[2].value);
        server.server = spec;
        server
            .apps
            .set_enabled_for(app, self.fields[4].value == "true");
        Ok(server)
    }

    pub fn save(&self, state: &AppState) -> Result<String, String> {
        match &self.kind {
            Kind::Mcp { original, .. } => {
                let server = self.mcp_value()?;
                cc_switch_core::block_on(async {
                    let _guards =
                        cc_switch_core::services::McpService::lock_live_sync_apps(state).await;
                    let current = state
                        .db
                        .get_mcp_server_rows()
                        .map_err(|e| e.to_string())?
                        .get(&server.id)
                        .map_err(|e| e.to_string())?
                        .cloned();
                    if serde_json::to_value(&current).unwrap()
                        != serde_json::to_value(original).unwrap()
                    {
                        return Err("MCP 已被其他窗口修改，请返回并刷新后编辑".into());
                    }
                    cc_switch_core::services::McpService::upsert_server(state, server)
                        .map_err(|e| e.to_string())
                })?;
            }
            Kind::Prompt { expected, id, app } => {
                let name = self.fields[0].value.trim();
                if name.is_empty() {
                    return Err("提示词名称不能为空".into());
                }
                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs() as i64;
                let prompt = Prompt {
                    id: id.clone(),
                    name: name.into(),
                    description: nonempty(&self.fields[1].value),
                    content: self.fields[2].value.clone(),
                    enabled: expected.prompt.as_ref().is_some_and(|p| p.enabled),
                    created_at: expected
                        .prompt
                        .as_ref()
                        .and_then(|p| p.created_at)
                        .or(Some(now)),
                    updated_at: Some(now),
                };
                PromptService::upsert_prompt_checked(
                    state,
                    app.clone(),
                    id,
                    prompt,
                    Some(expected),
                )
                .map_err(|e| e.to_string())?;
            }
            Kind::Repo { original } => {
                let repo = SkillRepo {
                    owner: self.fields[0].value.trim().into(),
                    name: self.fields[1].value.trim().into(),
                    branch: self.fields[2].value.trim().into(),
                    enabled: self.fields[3].value == "true",
                };
                SkillService::validate_repo_ref(&repo.owner, &repo.name, &repo.branch)
                    .map_err(|e| e.to_string())?;
                let _guard = cc_switch_core::services::skill::skill_state_write_guard();
                let current = state
                    .db
                    .get_skill_repos()
                    .map_err(|e| e.to_string())?
                    .into_iter()
                    .find(|r| r.owner == repo.owner && r.name == repo.name);
                if serde_json::to_value(&current).unwrap()
                    != serde_json::to_value(original).unwrap()
                {
                    return Err("仓库已存在或被其他窗口修改，请刷新".into());
                }
                state.db.save_skill_repo(&repo).map_err(|e| e.to_string())?;
            }
            Kind::Zip => return Err("ZIP 安装需要指定应用".into()),
        }
        Ok("已保存".into())
    }
}

pub(super) fn parse_json(text: &str) -> Result<serde_json::Value, String> {
    serde_json::from_str(text)
        .map_err(|e| format!("JSON 格式错误：第 {} 行，第 {} 列", e.line(), e.column()))
}

fn nonempty(text: &str) -> Option<String> {
    (!text.trim().is_empty()).then(|| text.trim().to_owned())
}
