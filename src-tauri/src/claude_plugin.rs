use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

use crate::error::AppError;

const CLAUDE_DIR: &str = ".claude";
const CLAUDE_CONFIG_FILE: &str = "config.json";
static CONFIG_WRITE_LOCK: Mutex<()> = Mutex::new(());

fn claude_dir() -> Result<PathBuf, AppError> {
    // 优先使用设置中的覆盖目录
    if let Some(dir) = crate::settings::get_override_dir(&crate::app_config::AppType::Claude) {
        return Ok(dir);
    }
    let home = dirs::home_dir().ok_or_else(|| AppError::Config("无法获取用户主目录".into()))?;
    Ok(home.join(CLAUDE_DIR))
}

pub fn claude_config_path() -> Result<PathBuf, AppError> {
    Ok(claude_dir()?.join(CLAUDE_CONFIG_FILE))
}

pub fn ensure_claude_dir_exists() -> Result<PathBuf, AppError> {
    let dir = claude_dir()?;
    if !dir.exists() {
        fs::create_dir_all(&dir).map_err(|e| AppError::io(&dir, e))?;
    }
    Ok(dir)
}

pub fn read_claude_config() -> Result<Option<String>, AppError> {
    let path = claude_config_path()?;
    match fs::read_to_string(&path) {
        Ok(content) => Ok(Some(content)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(AppError::io(&path, error)),
    }
}

fn is_managed_config(content: &str) -> bool {
    match serde_json::from_str::<serde_json::Value>(content) {
        Ok(value) => value
            .get("primaryApiKey")
            .and_then(|v| v.as_str())
            .map(|val| val == "any")
            .unwrap_or(false),
        Err(_) => false,
    }
}

pub fn write_claude_config() -> Result<bool, AppError> {
    update_config(&claude_config_path()?, false)
}

pub fn clear_claude_config() -> Result<bool, AppError> {
    update_config(&claude_config_path()?, true)
}

fn update_config(path: &Path, clear: bool) -> Result<bool, AppError> {
    let _guard = CONFIG_WRITE_LOCK
        .lock()
        .unwrap_or_else(|error| error.into_inner());
    let mut value = match fs::read_to_string(path) {
        Ok(content) => serde_json::from_str::<serde_json::Value>(&content)
            .map_err(|error| AppError::Config(format!("{}: {error}", path.display())))?,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            if clear {
                return Ok(false);
            }
            serde_json::json!({})
        }
        Err(error) => return Err(AppError::io(path, error)),
    };
    let object = value.as_object_mut().ok_or_else(|| {
        AppError::Config(format!("{} must contain a JSON object", path.display()))
    })?;
    let managed = object.get("primaryApiKey").and_then(|value| value.as_str()) == Some("any");
    if clear {
        if !managed {
            return Ok(false);
        }
        object.remove("primaryApiKey");
    } else {
        if managed {
            return Ok(false);
        }
        object.insert(
            "primaryApiKey".into(),
            serde_json::Value::String("any".into()),
        );
    }
    crate::config::write_json_file_private(path, &value)?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn invalid_configs_are_reported_and_preserved() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        for content in ["{broken", "[]", "null", "\"text\""] {
            fs::write(&path, content).unwrap();
            assert!(update_config(&path, false).is_err());
            assert!(update_config(&path, true).is_err());
            assert_eq!(fs::read_to_string(&path).unwrap(), content);
        }
    }

    #[test]
    fn only_managed_key_is_cleared_and_unrelated_fields_survive() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.json");
        let content = r#"{"primaryApiKey":"user-key","other":{"keep":true}}"#;
        fs::write(&path, content).unwrap();
        assert!(!update_config(&path, true).unwrap());
        assert_eq!(fs::read_to_string(&path).unwrap(), content);
        assert!(update_config(&path, false).unwrap());
        assert!(!update_config(&path, false).unwrap());
        assert!(update_config(&path, true).unwrap());
        let value: serde_json::Value =
            serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap();
        assert_eq!(value, serde_json::json!({"other":{"keep":true}}));
    }
}

pub fn claude_config_status() -> Result<(bool, PathBuf), AppError> {
    let path = claude_config_path()?;
    Ok((path.exists(), path))
}

pub fn is_claude_config_applied() -> Result<bool, AppError> {
    match read_claude_config()? {
        Some(content) => Ok(is_managed_config(&content)),
        None => Ok(false),
    }
}
