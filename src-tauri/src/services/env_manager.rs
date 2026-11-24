use super::env_checker::EnvConflict;
use chrono::Utc;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

mod legacy;
mod snapshot;

static ENV_MUTATION_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BackupInfo {
    pub backup_path: String,
    pub timestamp: String,
    pub conflicts: Vec<EnvConflict>,
    #[serde(default, skip_serializing)]
    snapshots: Vec<snapshot::EnvSnapshot>,
}

/// Delete environment variables with automatic backup
pub fn delete_env_vars(conflicts: Vec<EnvConflict>) -> Result<BackupInfo, String> {
    let _guard = ENV_MUTATION_LOCK
        .lock()
        .map_err(|_| "环境变量操作锁不可用".to_string())?;
    delete_env_vars_in(conflicts, &backup_dir()?)
}

fn delete_env_vars_in(
    conflicts: Vec<EnvConflict>,
    backup_dir: &Path,
) -> Result<BackupInfo, String> {
    let snapshots = snapshot::prepare_deletions(&conflicts)?;
    let backup_info = create_backup_in(backup_dir, &conflicts, snapshots)?;

    // Step 2: Delete variables
    for snapshot in &backup_info.snapshots {
        match snapshot.apply(false) {
            Ok(_) => {}
            Err(e) => {
                // If deletion fails, we keep the backup but return error
                return Err(format!(
                    "删除环境变量失败: {}. 备份已保存到: {}",
                    e, backup_info.backup_path
                ));
            }
        }
    }

    Ok(backup_info)
}

fn create_backup_in(
    backup_dir: &Path,
    conflicts: &[EnvConflict],
    snapshots: Vec<snapshot::EnvSnapshot>,
) -> Result<BackupInfo, String> {
    fs::create_dir_all(backup_dir).map_err(|e| format!("创建备份目录失败: {e}"))?;

    // 秒级时间戳 + 随机后缀，避免同一秒内多次清理互相覆盖；create_new 防止截断已有备份。
    let timestamp = Utc::now().format("%Y%m%d_%H%M%S").to_string();
    let nonce = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let backup_file = backup_dir.join(format!(
        "env-backup-{timestamp}-{nonce}-{}.json",
        std::process::id()
    ));

    // Create backup data
    let backup_info = BackupInfo {
        backup_path: backup_file.to_string_lossy().to_string(),
        timestamp: timestamp.clone(),
        conflicts: conflicts.to_vec(),
        snapshots,
    };

    // Write backup file
    let mut backup_data = serde_json::to_value(&backup_info)
        .map_err(|error| format!("序列化备份数据失败: {error}"))?;
    backup_data["snapshots"] = serde_json::to_value(&backup_info.snapshots)
        .map_err(|error| format!("序列化环境快照失败: {error}"))?;
    let json = serde_json::to_string_pretty(&backup_data)
        .map_err(|e| format!("序列化备份数据失败: {e}"))?;

    let mut options = fs::OpenOptions::new();
    options.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options
        .open(&backup_file)
        .map_err(|e| format!("创建备份文件失败: {e}"))?;
    if let Err(error) = file
        .write_all(json.as_bytes())
        .and_then(|_| file.sync_all())
    {
        drop(file);
        let _ = fs::remove_file(&backup_file);
        return Err(format!("写入备份文件失败: {error}"));
    }

    Ok(backup_info)
}

/// Get backup directory path
pub(crate) fn backup_dir() -> Result<PathBuf, String> {
    let home = dirs::home_dir().ok_or("无法获取用户主目录")?;
    Ok(home.join(".cc-switch").join("backups"))
}

/// Delete a single environment variable
#[cfg(any(test, target_os = "windows"))]
fn registry_is_current_user(source_path: &str) -> Result<bool, String> {
    if source_path.eq_ignore_ascii_case("HKEY_CURRENT_USER\\Environment") {
        Ok(true)
    } else if source_path.eq_ignore_ascii_case(
        "HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
    ) {
        Ok(false)
    } else {
        Err(format!("不支持的环境变量注册表路径: {source_path}"))
    }
}

#[cfg(any(test, not(target_os = "windows")))]
fn parse_file_source_path(source_path: &str) -> Result<(&str, usize), String> {
    let (file_path, line_num) = source_path
        .rsplit_once(':')
        .ok_or_else(|| "无效的文件路径格式".to_string())?;

    if file_path.is_empty() {
        return Err("无效的文件路径格式".to_string());
    }

    let line_num = line_num
        .parse::<usize>()
        .map_err(|_| "无效的文件路径格式".to_string())?;
    if line_num == 0 {
        return Err("无效的文件行号".to_string());
    }

    Ok((file_path, line_num))
}

/// Restore environment variables from backup
pub fn restore_from_backup(backup_path: String) -> Result<(), String> {
    let _guard = ENV_MUTATION_LOCK
        .lock()
        .map_err(|_| "环境变量操作锁不可用".to_string())?;
    // Read backup file
    let content = fs::read_to_string(&backup_path).map_err(|e| format!("读取备份文件失败: {e}"))?;

    let backup_info: BackupInfo =
        serde_json::from_str(&content).map_err(|e| format!("解析备份文件失败: {e}"))?;

    let snapshots = if backup_info.snapshots.is_empty() {
        legacy::prepare_restoration(&backup_info.conflicts)?
    } else {
        backup_info.snapshots
    };
    for snapshot in &snapshots {
        snapshot.validate(true)?;
    }
    for snapshot in &snapshots {
        snapshot.apply(true)?;
    }
    Ok(())
}

#[cfg(any(test, not(target_os = "windows")))]
fn shell_export_line(name: &str, value: &str) -> Result<String, String> {
    let valid_name = !name.is_empty()
        && name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        });
    if !valid_name || value.contains('\0') {
        return Err("Invalid environment variable name or value".to_string());
    }
    Ok(format!(
        "\nexport {name}='{}'\n",
        value.replace('\'', "'\\''")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_sources_must_match_supported_locations() {
        assert_eq!(
            registry_is_current_user("hkey_current_user\\environment"),
            Ok(true)
        );
        assert_eq!(
            registry_is_current_user("HKEY_LOCAL_MACHINE\\SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment"),
            Ok(false)
        );
        for path in [
            "",
            "prefix-HKEY_CURRENT_USER",
            "HKEY_CURRENT_USER\\Other",
            "HKEY_LOCAL_MACHINE",
        ] {
            assert!(registry_is_current_user(path).is_err());
        }
    }

    #[test]
    fn backups_are_complete_unique_and_private() {
        let temp = tempfile::tempdir().unwrap();
        let conflicts = vec![EnvConflict {
            var_name: "OPENAI_API_KEY".into(),
            var_value: "backup-secret".into(),
            source_type: "system".into(),
            source_path: "HKEY_CURRENT_USER\\Environment".into(),
        }];
        let first = create_backup_in(temp.path(), &conflicts, Vec::new()).unwrap();
        let second = create_backup_in(temp.path(), &conflicts, Vec::new()).unwrap();
        assert_ne!(first.backup_path, second.backup_path);
        let restored: BackupInfo =
            serde_json::from_slice(&fs::read(&first.backup_path).unwrap()).unwrap();
        assert_eq!(restored.conflicts[0].var_value, "backup-secret");
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            assert_eq!(
                fs::metadata(&first.backup_path)
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
    }

    #[test]
    fn snapshots_are_stored_in_backup_but_not_exposed_in_ipc_metadata() {
        let temp = tempfile::tempdir().unwrap();
        let backup = create_backup_in(
            temp.path(),
            &[],
            vec![snapshot::EnvSnapshot::File {
                path: temp.path().join("shellrc"),
                before: "private shell configuration".into(),
                after: String::new(),
            }],
        )
        .unwrap();
        let metadata = serde_json::to_string(&backup).unwrap();
        assert!(!metadata.contains("private shell configuration"));
        assert!(!metadata.contains("snapshots"));
        let stored: BackupInfo =
            serde_json::from_slice(&fs::read(&backup.backup_path).unwrap()).unwrap();
        assert_eq!(stored.snapshots.len(), 1);
    }

    #[test]
    fn legacy_backups_without_snapshots_remain_readable() {
        let backup: BackupInfo = serde_json::from_value(serde_json::json!({
            "backupPath": "legacy.json",
            "timestamp": "20260101_000000",
            "conflicts": []
        }))
        .unwrap();
        assert!(backup.snapshots.is_empty());
    }

    #[test]
    fn restore_preflights_every_snapshot_before_writing_any_file() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        fs::write(&first, "after").unwrap();
        fs::write(&second, "external edit").unwrap();
        let snapshots = [&first, &second]
            .into_iter()
            .map(|path| snapshot::EnvSnapshot::File {
                path: path.to_path_buf(),
                before: "before".into(),
                after: "after".into(),
            })
            .collect();
        let backup = create_backup_in(&temp.path().join("backups"), &[], snapshots).unwrap();
        assert!(restore_from_backup(backup.backup_path.clone()).is_err());
        assert_eq!(fs::read_to_string(&first).unwrap(), "after");
        assert_eq!(fs::read_to_string(&second).unwrap(), "external edit");
        fs::write(&second, "after").unwrap();
        restore_from_backup(backup.backup_path.clone()).unwrap();
        restore_from_backup(backup.backup_path).unwrap();
        assert_eq!(fs::read_to_string(first).unwrap(), "before");
        assert_eq!(fs::read_to_string(second).unwrap(), "before");
    }

    #[test]
    fn shell_restore_quotes_values_without_executing_them() {
        assert_eq!(
            shell_export_line("OPENAI_API_KEY", "a'b $HOME\n$(id)").unwrap(),
            "\nexport OPENAI_API_KEY='a'\\''b $HOME\n$(id)'\n"
        );
        for name in ["", "1KEY", "KEY;id", "KEY\nOTHER"] {
            assert!(shell_export_line(name, "value").is_err());
        }
    }

    #[cfg(not(target_os = "windows"))]
    use tempfile::tempdir;

    #[test]
    fn test_backup_dir_creation() {
        let backup_dir = backup_dir();
        assert!(backup_dir.is_ok());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn file_source_path_parser_preserves_colons_in_path() {
        assert_eq!(
            parse_file_source_path("/tmp/cc:test/.zshrc:8"),
            Ok(("/tmp/cc:test/.zshrc", 8))
        );
        assert!(parse_file_source_path("/tmp/.zshrc:not-a-line").is_err());
        assert!(parse_file_source_path("/tmp/.zshrc:0").is_err());
    }

    #[cfg(not(target_os = "windows"))]
    #[test]
    fn delete_and_restore_file_env_with_colon_in_path() {
        let temp = tempdir().expect("create temp directory");
        let colon_dir = temp.path().join("foo:bar");
        fs::create_dir(&colon_dir).expect("create colon directory");
        let file_path = colon_dir.join(".zshrc");
        fs::write(
            &file_path,
            "# keep\r\nexport OPENAI_API_KEY=original\r\nexport OTHER=1\r\n",
        )
        .expect("write shell config");

        let conflict = EnvConflict {
            var_name: "OPENAI_API_KEY".to_string(),
            var_value: "original".to_string(),
            source_type: "file".to_string(),
            source_path: format!("{}:2", file_path.display()),
        };

        let backup = delete_env_vars_in(vec![conflict], &temp.path().join("backups"))
            .expect("delete variable from shell config");
        let deleted = fs::read_to_string(&file_path).expect("read updated shell config");
        assert_eq!(deleted, "# keep\r\nexport OTHER=1\r\n");

        restore_from_backup(backup.backup_path).expect("restore variable to shell config");
        let restored = fs::read_to_string(&file_path).expect("read restored shell config");
        assert_eq!(
            restored,
            "# keep\r\nexport OPENAI_API_KEY=original\r\nexport OTHER=1\r\n"
        );
    }
}
