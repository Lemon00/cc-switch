use crate::app_config::{McpApps, McpServer};
use crate::config::atomic_write_private;
use crate::error::AppError;
use crate::store::AppState;
use serde_json::{json, Value};
use std::{fs, path::Path, sync::Mutex};

static WRITE_LOCK: Mutex<()> = Mutex::new(());
const TRANSPORT_FIELDS: [&str; 7] = ["command", "args", "env", "cwd", "url", "headers", "type"];

fn should_sync_agy_mcp() -> bool {
    crate::agy_config::agy_dir().exists() || crate::agy_config::customizations_dir().exists()
}

fn validate_change(change: AgyChange<'_>) -> Result<(), AppError> {
    match change {
        AgyChange::Upsert(spec) => {
            super::validation::validate_server_spec(&unified_spec(spec))?;
            if super::validation::spec_transport_type(spec) == "sse" {
                return Err(AppError::Config(
                    "Antigravity supports stdio and streamable HTTP, not SSE".into(),
                ));
            }
            Ok(())
        }
        AgyChange::Disable | AgyChange::Remove => Ok(()),
    }
}

fn read(path: &Path) -> Result<Value, AppError> {
    let text = match fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(json!({"mcpServers": {}})),
        Err(e) => return Err(AppError::io(path, e)),
    };
    // agy 首次启动会建一个 0 字节的 mcp_config.json，当成没有配置
    if text.trim().is_empty() {
        return Ok(json!({"mcpServers": {}}));
    }
    let value: Value = serde_json::from_str(&text)
        .map_err(|_| AppError::Config("Invalid Antigravity MCP JSON".into()))?;
    if !value.is_object() || value.get("mcpServers").is_some_and(|v| !v.is_object()) {
        return Err(AppError::Config(
            "Invalid Antigravity mcpServers object".into(),
        ));
    }
    Ok(value)
}

#[derive(Clone, Copy)]
pub enum AgyChange<'a> {
    Upsert(&'a Value),
    Disable,
    Remove,
}
impl<'a> AgyChange<'a> {
    pub fn from_enabled(spec: Option<&'a Value>) -> Self {
        spec.map_or(Self::Disable, Self::Upsert)
    }
}

pub fn sync(id: &str, spec: Option<&Value>) -> Result<(), AppError> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::Message(e.to_string()))?;
    let change = AgyChange::from_enabled(spec);
    if !should_sync_agy_mcp() {
        return validate_change(change);
    }
    sync_file(
        &crate::agy_config::customizations_dir().join("mcp_config.json"),
        id,
        change,
    )
}

pub fn sync_and_commit<T>(
    id: &str,
    change: AgyChange<'_>,
    commit: impl FnOnce() -> Result<T, AppError>,
) -> Result<T, AppError> {
    let _guard = WRITE_LOCK
        .lock()
        .map_err(|e| AppError::Message(e.to_string()))?;
    if !should_sync_agy_mcp() {
        validate_change(change)?;
        return commit();
    }
    let path = crate::agy_config::customizations_dir().join("mcp_config.json");
    crate::sync_utils::write_and_commit(&path, || sync_file(&path, id, change), commit)
}

fn sync_file(path: &Path, id: &str, change: AgyChange<'_>) -> Result<(), AppError> {
    // Disable/Remove on a missing file must not create mcp_config.json.
    if matches!(change, AgyChange::Disable | AgyChange::Remove) && !path.exists() {
        return Ok(());
    }
    let mut document = read(path)?;
    if document.get("mcpServers").is_none() {
        document["mcpServers"] = json!({});
    }
    let servers = document["mcpServers"].as_object_mut().unwrap();
    if let AgyChange::Upsert(spec) = change {
        let normalized = unified_spec(spec);
        super::validation::validate_server_spec(&normalized)?;
        if super::validation::spec_transport_type(spec) == "sse" {
            return Err(AppError::Config(
                "Antigravity supports stdio and streamable HTTP, not SSE".into(),
            ));
        }
        let mut merged = servers.get(id).cloned().unwrap_or_else(|| json!({}));
        let object = merged
            .as_object_mut()
            .ok_or_else(|| AppError::Config("Invalid Antigravity MCP entry".into()))?;
        for field in TRANSPORT_FIELDS {
            object.remove(field);
        }
        object.extend(normalized.as_object().unwrap().clone());
        object.remove("enabled");
        object.remove("disabled");
        object.remove("serverUrl");
        object.remove("type");
        object.insert("disabled".into(), json!(false));
        servers.insert(id.into(), merged);
    } else if matches!(change, AgyChange::Remove) {
        if !servers.contains_key(id) {
            return Ok(());
        }
        servers.remove(id);
    } else if !servers.contains_key(id) {
        return Ok(());
    } else if let Some(server) = servers.get_mut(id) {
        let object = server
            .as_object_mut()
            .ok_or_else(|| AppError::Config("Invalid Antigravity MCP entry".into()))?;
        object.insert("disabled".into(), json!(true));
    }
    atomic_write_private(
        path,
        serde_json::to_string_pretty(&document)
            .map_err(|e| AppError::Config(e.to_string()))?
            .as_bytes(),
    )
}

pub fn import(state: &AppState) -> Result<usize, AppError> {
    let document = read(&crate::agy_config::customizations_dir().join("mcp_config.json"))?;
    let (existing, unreadable) = state
        .db
        .get_mcp_server_rows()?
        .into_readable_and_unreadable_ids();
    let mut count = 0;
    let mut skipped = Vec::new();
    for (id, native) in document["mcpServers"].as_object().into_iter().flatten() {
        if unreadable.contains(id) {
            skipped.push(format!("'{id}': existing database row is unreadable"));
            continue;
        }
        let mut spec = unified_spec(native);
        if validate_change(AgyChange::Upsert(&spec)).is_err() {
            skipped.push(format!("'{id}': invalid transport configuration"));
            continue;
        }
        let enabled = native
            .get("disabled")
            .and_then(Value::as_bool)
            .map(|disabled| !disabled)
            .unwrap_or(true);
        spec.as_object_mut().unwrap().remove("disabled");
        let server = if let Some(mut server) = existing.get(id).cloned() {
            if transport_spec(&server.server) != transport_spec(&spec) {
                skipped.push(format!("'{id}': conflicts with an existing server"));
                continue;
            }
            server.apps.agy = enabled;
            server
        } else {
            count += 1;
            McpServer {
                id: id.clone(),
                name: id.clone(),
                server: spec,
                apps: McpApps {
                    agy: enabled,
                    ..Default::default()
                },
                description: None,
                homepage: None,
                docs: None,
                tags: vec![],
            }
        };
        state.db.save_mcp_server(&server)?;
    }
    super::import_outcome(count, "Antigravity", skipped)
}

fn transport_spec(spec: &Value) -> Value {
    let mut spec = unified_spec(spec);
    if let Some(object) = spec.as_object_mut() {
        object.retain(|key, _| TRANSPORT_FIELDS.contains(&key.as_str()));
    }
    spec
}

fn unified_spec(native: &Value) -> Value {
    let mut spec = native.clone();
    if let Some(url) = spec.get("serverUrl").cloned() {
        if spec.get("url").is_none() {
            spec["url"] = url;
        }
        spec.as_object_mut().unwrap().remove("serverUrl");
    }
    if spec.is_object() && super::validation::spec_transport_type(&spec) == "http" {
        spec["type"] = json!("http");
    } else if spec.is_object() && spec.get("type").is_none() && spec.get("command").is_some() {
        spec["type"] = json!("stdio");
    }
    spec
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn agy_mcp_preserves_options_and_distinguishes_disable_from_remove() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        fs::write(&path, r#"{"extra":1,"mcpServers":{"unmanaged":{"command":"keep"},"managed":{"command":"old","args":["old"],"timeout":3000}}}"#).unwrap();
        sync_file(&path,"managed",AgyChange::Upsert(&json!({"type":"http","url":"https://example.test/mcp","headers":{"X-Test":"test"}}))).unwrap();
        let doc = read(&path).unwrap();
        assert_eq!(doc["extra"], 1);
        assert_eq!(doc["mcpServers"]["unmanaged"]["command"], "keep");
        let server = &doc["mcpServers"]["managed"];
        assert_eq!(server["timeout"], 3000);
        assert_eq!(server["disabled"], false);
        assert_eq!(server["headers"]["X-Test"], "test");
        assert!(server.get("command").is_none());
        assert!(server.get("args").is_none());
        assert!(server.get("type").is_none());
        sync_file(&path, "managed", AgyChange::Disable).unwrap();
        assert_eq!(
            read(&path).unwrap()["mcpServers"]["managed"]["disabled"],
            true
        );
        sync_file(&path, "managed", AgyChange::Remove).unwrap();
        assert!(read(&path).unwrap()["mcpServers"].get("managed").is_none());
    }
    #[test]
    fn agy_rejects_broken_config_and_sse_without_writing() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        for original in [
            "{broken",
            r#"{"mcpServers":[]}"#,
            r#"{"mcpServers":{"m":null}}"#,
        ] {
            fs::write(&path, original).unwrap();
            assert!(sync_file(&path, "m", AgyChange::Disable).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), original);
        }
        fs::write(&path, "{}").unwrap();
        assert!(sync_file(
            &path,
            "m",
            AgyChange::Upsert(&json!({"type":"sse","url":"https://example.test/sse"}))
        )
        .is_err());
        assert_eq!(fs::read_to_string(&path).unwrap(), "{}");
    }
    #[test]
    fn agy_treats_the_empty_file_agy_creates_as_no_servers() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        fs::write(&path, "").unwrap();
        sync_file(&path, "m", AgyChange::Upsert(&json!({"command":"new"}))).unwrap();
        assert_eq!(read(&path).unwrap()["mcpServers"]["m"]["command"], "new");
    }
    #[test]
    fn agy_mcp_restores_native_file_when_database_commit_fails() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("mcp_config.json");
        let original = r#"{"mcpServers":{"m":{"command":"old"}}}"#;
        fs::write(&path, original).unwrap();
        let result: Result<(), AppError> = crate::sync_utils::write_and_commit(
            &path,
            || sync_file(&path, "m", AgyChange::Upsert(&json!({"command":"new"}))),
            || Err(AppError::Database("fixture failure".into())),
        );
        assert!(result.is_err());
        assert_eq!(fs::read_to_string(path).unwrap(), original);
    }
    #[test]
    fn agy_normalizes_legacy_url_for_import() {
        let normalized =
            unified_spec(&json!({"serverUrl":"https://example.test/mcp","disabled":true}));
        assert_eq!(normalized["url"], "https://example.test/mcp");
        assert_eq!(normalized["type"], "http");
        assert!(normalized.get("serverUrl").is_none());
    }
}
