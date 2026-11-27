use crate::error::AppError;
use chrono::Utc;
use std::fs;
use std::io::Write;
use std::path::Path;

const MAX_BACKUPS: usize = 10;
static CONFIG_BACKUP_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// 配置导入导出相关业务逻辑
pub struct ConfigService;

impl ConfigService {
    /// 为当前 config.json 创建备份，返回备份 ID（若文件不存在则返回空字符串）。
    pub fn create_backup(config_path: &Path) -> Result<String, AppError> {
        let _guard = CONFIG_BACKUP_LOCK
            .lock()
            .map_err(|_| AppError::Config("Config backup lock unavailable".into()))?;
        let contents = match fs::read(config_path) {
            Ok(contents) => contents,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(String::new()),
            Err(error) => return Err(AppError::io(config_path, error)),
        };

        let timestamp = Utc::now().format("%Y%m%d_%H%M%S");
        let backup_id = format!("backup_{timestamp}_{}", uuid::Uuid::new_v4());

        let backup_dir = config_path
            .parent()
            .ok_or_else(|| AppError::Config("Invalid config path".into()))?
            .join("backups");

        fs::create_dir_all(&backup_dir).map_err(|e| AppError::io(&backup_dir, e))?;

        let backup_path = backup_dir.join(format!("{backup_id}.json"));
        let mut staged = tempfile::NamedTempFile::new_in(&backup_dir)
            .map_err(|error| AppError::io(&backup_dir, error))?;
        staged
            .write_all(&contents)
            .map_err(|error| AppError::io(&backup_path, error))?;
        staged
            .as_file()
            .sync_all()
            .map_err(|error| AppError::io(&backup_path, error))?;
        staged
            .persist_noclobber(&backup_path)
            .map_err(|error| AppError::io(&backup_path, error.error))?;

        Self::cleanup_old_backups(&backup_dir, MAX_BACKUPS)?;

        Ok(backup_id)
    }

    fn cleanup_old_backups(backup_dir: &Path, retain: usize) -> Result<(), AppError> {
        if retain == 0 {
            return Ok(());
        }

        let entries = match fs::read_dir(backup_dir) {
            Ok(iter) => iter
                .filter_map(|entry| entry.ok())
                .filter(|entry| {
                    let name = entry.file_name();
                    let name = name.to_string_lossy();
                    name.starts_with("backup_")
                        && name.ends_with(".json")
                        && entry.file_type().is_ok_and(|kind| kind.is_file())
                })
                .collect::<Vec<_>>(),
            Err(_) => return Ok(()),
        };

        if entries.len() <= retain {
            return Ok(());
        }

        let remove_count = entries.len().saturating_sub(retain);
        let mut sorted = entries;

        sorted.sort_by(|a, b| {
            let a_time = a.metadata().and_then(|m| m.modified()).ok();
            let b_time = b.metadata().and_then(|m| m.modified()).ok();
            a_time.cmp(&b_time)
        });

        for entry in sorted.into_iter().take(remove_count) {
            if let Err(err) = fs::remove_file(entry.path()) {
                log::warn!(
                    "Failed to remove old backup {}: {}",
                    entry.path().display(),
                    err
                );
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn consecutive_backups_are_distinct_and_cleanup_preserves_other_categories() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("config.json");
        let backups = temp.path().join("backups");
        fs::create_dir(&backups).unwrap();
        fs::write(backups.join("env-backup-important.json"), "environment").unwrap();
        fs::write(backups.join("manual-important.json"), "manual").unwrap();
        fs::write(&path, "first").unwrap();
        let first = ConfigService::create_backup(&path).unwrap();
        fs::write(&path, "second").unwrap();
        let second = ConfigService::create_backup(&path).unwrap();
        assert_ne!(first, second);
        assert_eq!(
            fs::read_to_string(backups.join(format!("{first}.json"))).unwrap(),
            "first"
        );
        for _ in 0..12 {
            ConfigService::create_backup(&path).unwrap();
        }
        assert_eq!(fs::read_dir(&backups).unwrap().count(), MAX_BACKUPS + 2);
        assert_eq!(
            fs::read_to_string(backups.join("env-backup-important.json")).unwrap(),
            "environment"
        );
        assert_eq!(
            fs::read_to_string(backups.join("manual-important.json")).unwrap(),
            "manual"
        );
    }
}
