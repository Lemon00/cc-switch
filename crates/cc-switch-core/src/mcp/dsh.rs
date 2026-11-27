use crate::app_config::{McpApps, McpServer};
use crate::error::AppError;
use crate::store::AppState;
use serde_json::{json, Value};

use super::validation::spec_transport_type;

const TRANSPORT_COMPARE_FIELDS: [&str; 6] = ["command", "args", "env", "cwd", "url", "headers"];

fn transport_matches(existing: &Value, imported: &Value) -> bool {
    spec_transport_type(existing) == spec_transport_type(imported)
        && TRANSPORT_COMPARE_FIELDS
            .iter()
            .all(|field| existing.get(*field) == imported.get(*field))
}

pub fn sync(id: &str, spec: Option<&Value>) -> Result<(), AppError> {
    sync_and_commit(id, spec, || Ok(()))
}

pub fn sync_and_commit<T>(
    id: &str,
    spec: Option<&Value>,
    commit: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    crate::dsh_config::sync_mcp_and_commit(id, spec, commit)
}

pub fn import(state: &AppState) -> Result<usize, AppError> {
    let (mut existing, unreadable) = state
        .db
        .get_mcp_server_rows()?
        .into_readable_and_unreadable_ids();
    let mut count = 0;
    let mut skipped = Vec::new();
    for entry in crate::dsh_config::mcp_entries()? {
        let Some(id) = entry["config"]["serverName"].as_str() else {
            continue;
        };
        if unreadable.iter().any(|unreadable_id| unreadable_id == id) {
            skipped.push(format!("'{id}': existing database row is unreadable"));
            continue;
        }
        let Ok(mut spec) = serde_json::to_value(&entry["config"]) else {
            continue;
        };
        let Some(object) = spec.as_object_mut() else {
            continue;
        };
        let transport = object.remove("transport").unwrap_or(Value::Null);
        object.remove("serverName");
        object.insert(
            "type".into(),
            match transport.as_str() {
                Some("stdio") => json!("stdio"),
                Some("streamable-http") => json!("http"),
                _ => continue,
            },
        );
        if super::validation::validate_server_spec(&spec).is_err() {
            continue;
        }
        let mut server = if let Some(server) = existing.get(id).cloned() {
            // Import never overwrites a different server's transport or managed state.
            if !transport_matches(&server.server, &spec) {
                skipped.push(format!("'{id}': conflicts with an existing server"));
                continue;
            }
            server
        } else {
            count += 1;
            McpServer {
                id: id.into(),
                name: id.into(),
                server: spec,
                apps: McpApps::default(),
                description: None,
                homepage: None,
                docs: None,
                tags: vec![],
            }
        };
        server.apps.dsh = entry["disabled"].as_bool() != Some(true);
        state.db.save_mcp_server(&server)?;
        existing.insert(id.to_string(), server);
    }
    super::import_outcome(count, "DSH", skipped)
}
