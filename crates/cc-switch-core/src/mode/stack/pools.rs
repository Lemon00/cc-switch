//! The public catalog and request routing share the same model pools.
//! Only exact upstream model identities are combined; display aliases never
//! decide whether two accounts can answer the same request.

use super::*;
use std::collections::HashMap;

#[derive(Debug)]
pub struct ModelPool {
    pub model: String,
    pub id: String,
    pub members: Vec<usize>,
    pub one_m: bool,
    pub window: u64,
}

/// `group` is reserved. Escape it (and the escape namespace) injectively so
/// even an existing account named group keeps a separate, stable identity.
pub fn account_namespace(key: &str) -> String {
    if key == "group" || key.starts_with("account-") || key.starts_with("ccs-") {
        format!("account-{key}")
    } else {
        key.to_string()
    }
}

pub fn model_pools(app: &AppType, members: &[Member]) -> Vec<ModelPool> {
    let mut pools: Vec<ModelPool> = Vec::new();
    let mut by_model = HashMap::new();
    for (index, member) in members.iter().enumerate() {
        let models: Vec<(String, bool, u64)> = match app {
            AppType::Claude => claude_models(&member.key, &member.provider)
                .into_iter()
                .map(|m| {
                    (
                        strip_one_m_suffix_for_upstream(&m.upstream)
                            .trim()
                            .to_string(),
                        m.one_m,
                        if m.one_m { 1_000_000 } else { m.window },
                    )
                })
                .collect(),
            AppType::Codex => {
                // Official requests use the client's own account credentials;
                // they cannot share a retry chain with API-key accounts.
                if crate::services::provider::codex_direct::is_official(&member.provider)
                    || crate::services::provider::codex_direct::check_stack_member(&member.provider)
                        .is_err()
                {
                    continue;
                }
                codex_model_ids(&member.key, &member.provider)
                    .iter()
                    .filter_map(|id| match decode(app, id) {
                        Decoded::Stack { model, .. } => Some((model.to_string(), false, 0)),
                        _ => None,
                    })
                    .collect()
            }
            _ => continue,
        };
        for (model, one_m, window) in models {
            if let Some(&at) = by_model.get(&model) {
                let pool: &mut ModelPool = &mut pools[at];
                if !pool.members.contains(&index) {
                    pool.members.push(index);
                    pool.one_m &= one_m;
                    pool.window = pool.window.min(window);
                }
            } else {
                by_model.insert(model.clone(), pools.len());
                pools.push(ModelPool {
                    model,
                    id: String::new(),
                    members: vec![index],
                    one_m,
                    window,
                });
            }
        }
    }
    for pool in &mut pools {
        let namespace = if pool.members.len() > 1 {
            "group".to_string()
        } else {
            account_namespace(&members[pool.members[0]].key)
        };
        let marker = if pool.one_m {
            ONE_M_MARKER_FOR_CLIENT
        } else {
            ""
        };
        pool.id = format!("{namespace}/{}{marker}", pool.model);
    }
    pools
}

/// Rewrite the assembled Codex catalog once, after each account's protocol
/// profile and limits have been applied. All third-party rows use intermediate
/// per-account ids, including the default. Native official rows stay native.
pub fn group_codex_catalog(catalog: &mut Value, members: &[Member]) {
    let pools = model_pools(&AppType::Codex, members);
    let Some(rows) = catalog.get_mut("models").and_then(Value::as_array_mut) else {
        return;
    };
    let mut published: Vec<Value> = Vec::new();
    let mut seen = HashMap::new();
    for mut row in std::mem::take(rows) {
        let Some(slug) = row.get("slug").and_then(Value::as_str) else {
            continue;
        };
        let pool = pools.iter().find(|pool| {
            pool.members.iter().any(|&index| {
                let member = &members[index];
                match decode(&AppType::Codex, slug) {
                    Decoded::Stack { key, model, .. } => key == member.key && model == pool.model,
                    _ => false,
                }
            })
        });
        if let Some(pool) = pool {
            row["slug"] = Value::String(pool.id.clone());
            row["display_name"] = Value::String(pool.id.clone());
            // Used by catalog import to avoid saving aggregated rows to an account.
            row["cc_switch_aggregate"] = Value::Bool(true);
            // A self-review reference must use the newly published model id too.
            if row
                .get("auto_review_model_override")
                .and_then(Value::as_str)
                == Some(pool.model.as_str())
            {
                row["auto_review_model_override"] = Value::String(pool.id.clone());
            }
            if let Some(&index) = seen.get(&pool.id) {
                intersect_capabilities(&mut published[index], &row);
                continue;
            }
            seen.insert(pool.id.clone(), published.len());
        }
        published.push(row);
    }
    for (index, row) in published.iter_mut().enumerate() {
        row["priority"] = Value::from(index + 1);
        if row.get("cc_switch_aggregate").and_then(Value::as_bool) == Some(true) {
            let window = row
                .get("context_window")
                .and_then(Value::as_u64)
                .unwrap_or(128_000);
            let id = row["slug"].as_str().unwrap_or_default();
            row["description"] = Value::String(model_description(id, window));
        }
    }
    *rows = published;
}

fn intersect_capabilities(row: &mut Value, other: &Value) {
    // A group can span native Responses and converted Chat/Anthropic. Use
    // ordinary function tools when the members disagree on the tool protocol.
    if ["tool_mode", "shell_type", "apply_patch_tool_type"]
        .iter()
        .any(|key| row[*key] != other[*key])
    {
        if let Some(object) = row.as_object_mut() {
            for key in [
                "tool_mode",
                "apply_patch_tool_type",
                "web_search_tool_type",
                "tools",
                "model_messages",
            ] {
                object.remove(key);
            }
        }
        row["shell_type"] = Value::String("shell_command".to_string());
        row["comp_hash"] = Value::String("cc-switch".to_string());
    }
    for key in [
        "context_window",
        "max_context_window",
        "auto_compact_token_limit",
        "max_output_tokens",
    ] {
        if let (Some(a), Some(b)) = (row[key].as_u64(), other[key].as_u64()) {
            row[key] = Value::from(a.min(b));
        }
    }
    for key in ["input_modalities", "supported_reasoning_levels"] {
        if let Some(values) = row.get_mut(key).and_then(Value::as_array_mut) {
            values.retain(|value| {
                other[key].as_array().is_some_and(|others| {
                    others.iter().any(|candidate| {
                        if key == "supported_reasoning_levels" {
                            candidate.get("effort") == value.get("effort")
                        } else {
                            candidate == value
                        }
                    })
                })
            });
        }
    }
    if let Some(default) = row.get("default_reasoning_level").and_then(Value::as_str) {
        if !row["supported_reasoning_levels"]
            .as_array()
            .is_some_and(|levels| levels.iter().any(|l| l["effort"] == default))
        {
            row["default_reasoning_level"] = row["supported_reasoning_levels"]
                .as_array()
                .and_then(|levels| levels.first())
                .and_then(|l| l.get("effort"))
                .cloned()
                .unwrap_or(Value::Null);
        }
    }
    if let Some(object) = row.as_object_mut() {
        for (key, value) in object.iter_mut() {
            if value.is_boolean() && key != "cc_switch_aggregate" {
                *value = Value::Bool(
                    value.as_bool() == Some(true) && other[key].as_bool() == Some(true),
                );
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn member(id: &str, models: &[&str]) -> Member {
        let mut provider = Provider::with_id(
            id.to_string(),
            format!("Account {id}"),
            json!({
                "config": "model = \"shared\"\nmodel_provider = \"relay\"\n[model_providers.relay]\nbase_url = \"https://example.test/v1\"\nwire_api = \"responses\"\n",
                "auth": { "OPENAI_API_KEY": "test" },
                "modelCatalog": { "models": models.iter().map(|model| json!({ "model": model })).collect::<Vec<_>>() },
            }),
            None,
        );
        provider.meta = Some(crate::provider::ProviderMeta {
            stack_models: Some(
                models
                    .iter()
                    .map(|model| serde_json::from_value(json!({ "model": model })).unwrap())
                    .collect(),
            ),
            ..Default::default()
        });
        Member {
            provider,
            key: id.to_string(),
            route: id == "a",
            model_ids: vec![],
        }
    }

    #[test]
    fn shared_models_have_one_public_name_and_unique_models_keep_the_account() {
        let members = vec![
            member("a", &["shared", "only-a", "vendor/m"]),
            member("b", &["shared", "only-b", "vendor/m"]),
        ];
        for app in [AppType::Claude, AppType::Codex] {
            let pools = model_pools(&app, &members);
            let ids: Vec<_> = pools.iter().map(|p| p.id.as_str()).collect();
            assert_eq!(
                ids,
                ["group/shared", "a/only-a", "group/vendor/m", "b/only-b"]
            );
            assert_eq!(pools[0].members, [0, 1]);
        }
        for model in claude_published(&members) {
            assert!(!model.display_name.contains("Account"));
            assert!(!model.description.contains("Account"));
        }
        assert_eq!(claude_route_default(&members).unwrap().id, "group/shared");
    }

    #[test]
    fn reserved_account_names_do_not_collide() {
        let members = vec![
            member("group", &["unique"]),
            member("account-group", &["unique-2"]),
        ];
        let pools = model_pools(&AppType::Claude, &members);
        assert_eq!(pools[0].id, "account-group/unique");
        assert_eq!(pools[1].id, "account-account-group/unique-2");
    }

    #[test]
    fn common_claude_window_requires_every_account_to_support_it() {
        let mut members = vec![member("a", &["shared[1m]"]), member("b", &["shared"])];
        members[1].provider.settings_config["env"] =
            json!({ "CLAUDE_CODE_MAX_CONTEXT_TOKENS": "128000" });
        let models = claude_published(&members);
        assert_eq!(models.len(), 1);
        assert_eq!(models[0].id, "group/shared");
        assert!(!models[0].one_m);
        assert_eq!(models[0].window, 128_000);
    }

    #[test]
    fn codex_catalog_merges_default_account_and_intersects_capabilities() {
        let members = vec![member("a", &["shared", "only-a"]), member("b", &["shared"])];
        let mut catalog = json!({ "models": [
            { "slug": "ccs-a/shared", "display_name": "Account A", "description": "Account A", "context_window": 200000,
              "input_modalities": ["text", "image"], "default_reasoning_level": "high",
              "supported_reasoning_levels": [{"effort":"low"}, {"effort":"high"}], "supports_parallel_tool_calls": true },
            { "slug": "ccs-a/only-a", "context_window": 64000 },
            { "slug": "ccs-b/shared", "display_name": "Account B", "context_window": 128000,
              "input_modalities": ["text"], "supported_reasoning_levels": [{"effort":"low"}], "supports_parallel_tool_calls": false }
        ] });
        group_codex_catalog(&mut catalog, &members);
        let rows = catalog["models"].as_array().unwrap();
        assert_eq!(rows.len(), 2);
        assert_eq!(rows[0]["slug"], "group/shared");
        assert_eq!(rows[0]["display_name"], "group/shared");
        assert!(!rows[0]["description"].as_str().unwrap().contains("Account"));
        assert_eq!(rows[0]["context_window"], 128000);
        assert_eq!(rows[0]["input_modalities"], json!(["text"]));
        assert_eq!(rows[0]["default_reasoning_level"], "low");
        assert_eq!(rows[0]["supports_parallel_tool_calls"], false);
        assert_eq!(rows[1]["slug"], "a/only-a");
    }

    #[test]
    fn codex_shared_models_use_common_tools_across_protocols() {
        let members = vec![member("a", &["shared"]), member("b", &["shared"])];
        let mut catalog = json!({"models": [
            {"slug": "ccs-a/shared", "tool_mode": "responses", "shell_type": "local_shell",
             "apply_patch_tool_type": "custom", "web_search_tool_type": "native", "tools": ["custom"],
             "model_messages": {"instructions_template": "vendor"}, "use_responses_lite": true,
             "default_reasoning_level": "high", "supported_reasoning_levels": [{"effort":"high"}]},
            {"slug": "ccs-b/shared", "shell_type": "shell_command", "use_responses_lite": false,
             "supported_reasoning_levels": [{"effort":"low"}]}
        ]});
        group_codex_catalog(&mut catalog, &members);
        let rows = catalog["models"].as_array().unwrap();
        assert_eq!(rows.len(), 1);
        let model = &rows[0];
        assert_eq!(model["slug"], "group/shared");
        assert_eq!(model["shell_type"], "shell_command");
        for key in [
            "tool_mode",
            "apply_patch_tool_type",
            "web_search_tool_type",
            "tools",
            "model_messages",
        ] {
            assert!(model.get(key).is_none());
        }
        assert_eq!(model["use_responses_lite"], false);
        assert_eq!(model["supported_reasoning_levels"], json!([]));
        assert!(model["default_reasoning_level"].is_null());
    }
}
