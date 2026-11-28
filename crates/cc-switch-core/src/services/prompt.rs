use indexmap::IndexMap;
use std::path::Path;

use crate::app_config::AppType;
use crate::config::write_text_file;
use crate::error::AppError;
use crate::prompt::{Prompt, PromptWriteExpectation};
use crate::prompt_files::{
    prompt_file_path, read_text_file, validate_prompt_content,
    CONTENT_WRITE_LOCK as PROMPT_FILE_LOCK,
};

use crate::store::AppState;

/// 安全地获取当前 Unix 时间戳
fn get_unix_timestamp() -> Result<i64, AppError> {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .map_err(|e| AppError::Message(format!("Failed to get system time: {e}")))
}

pub struct PromptService;

fn project_prompt_set_to_path(
    prompts: &IndexMap<String, Prompt>,
    target_path: &Path,
) -> Result<Option<String>, AppError> {
    let enabled: Vec<(&String, &Prompt)> = prompts
        .iter()
        .filter(|(_, prompt)| prompt.enabled)
        .collect();

    if let Some((_, prompt)) = enabled.first() {
        write_text_file(target_path, &prompt.content)?;
    }
    // With nothing enabled, leave the target file untouched. This projection
    // only runs after a database restore, and the live file is not part of
    // the sync payload — clearing it here would wipe local content the
    // restored snapshot never contained. Disabling the last prompt from the
    // UI still clears the file via `PromptService::upsert_prompt`.

    if enabled.len() <= 1 {
        return Ok(None);
    }

    let ids = enabled
        .iter()
        .map(|(id, _)| id.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    Ok(Some(format!(
        "多个 Prompt 同时启用，已按稳定顺序投影第一个；enabled IDs: {ids}"
    )))
}

impl PromptService {
    pub fn get_prompts(
        state: &AppState,
        app: AppType,
    ) -> Result<IndexMap<String, Prompt>, AppError> {
        // A restore replaces the DB before projecting it to live files. Keep
        // backfill outside that window, including calls outside the UI command.
        let Ok(_sync_guard) = super::sync_protocol::sync_mutex().try_lock() else {
            return state.db.get_prompts(&app);
        };
        let _guard = PROMPT_FILE_LOCK
            .lock()
            .map_err(|error| AppError::Message(error.to_string()))?;
        let mut prompts = state.db.get_prompts(&app)?;
        // External editors change the live file without updating the saved
        // selection. Refresh only that selection; inactive templates are separate.
        if let Some(prompt) = prompts.values_mut().find(|prompt| prompt.enabled) {
            match Self::get_current_file_content(app.clone()) {
                Ok(Some(content)) if !content.trim().is_empty() && prompt.content != content => {
                    prompt.content = content;
                    prompt.updated_at = Some(get_unix_timestamp()?);
                    state.db.save_prompt(&app, prompt)?;
                }
                Ok(_) => {}
                Err(error) => log::warn!(
                    "Failed to refresh {} prompt from live file; keeping saved prompts: {error}",
                    app.as_str()
                ),
            }
        }
        Ok(prompts)
    }

    pub fn upsert_prompt(
        state: &AppState,
        app: AppType,
        id: &str,
        prompt: Prompt,
    ) -> Result<(), AppError> {
        Self::upsert_prompt_checked(state, app, id, prompt, None)
    }

    pub fn upsert_prompt_checked(
        state: &AppState,
        app: AppType,
        id: &str,
        prompt: Prompt,
        expected: Option<&PromptWriteExpectation>,
    ) -> Result<(), AppError> {
        upsert_file_prompt_checked(state, &app, id, prompt, &prompt_file_path(&app)?, expected)
    }

    pub fn delete_prompt(state: &AppState, app: AppType, id: &str) -> Result<(), AppError> {
        let _guard = PROMPT_FILE_LOCK
            .lock()
            .map_err(|error| AppError::Message(error.to_string()))?;
        let prompts = state.db.get_prompts(&app)?;

        if let Some(prompt) = prompts.get(id) {
            if prompt.enabled {
                return Err(AppError::InvalidInput("无法删除已启用的提示词".to_string()));
            }
        }

        state.db.delete_prompt(&app, id)?;
        Ok(())
    }

    pub fn enable_prompt(state: &AppState, app: AppType, id: &str) -> Result<(), AppError> {
        // 回填当前 live 文件内容到已启用的提示词，或创建备份
        enable_file_prompt(state, &app, id, &prompt_file_path(&app)?)
    }

    pub fn import_from_file(state: &AppState, app: AppType) -> Result<String, AppError> {
        let content = {
            let file_path = prompt_file_path(&app)?;
            read_text_file(&file_path)?
                .ok_or_else(|| AppError::Message("提示词文件不存在".to_string()))?
        };
        let timestamp = get_unix_timestamp()?;

        let id = format!("imported-{}", uuid::Uuid::new_v4());
        let prompt = Prompt {
            id: id.clone(),
            name: format!(
                "导入的提示词 {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            ),
            content,
            description: Some("从现有配置文件导入".to_string()),
            enabled: false,
            created_at: Some(timestamp),
            updated_at: Some(timestamp),
        };

        Self::upsert_prompt(state, app, &id, prompt)?;
        Ok(id)
    }

    pub fn get_current_file_content(app: AppType) -> Result<Option<String>, AppError> {
        let file_path = prompt_file_path(&app)?;
        read_text_file(&file_path)
    }

    /// Project the database SSOT to one application's managed prompt file.
    ///
    /// This deliberately does not call `enable_prompt`: restore paths must not
    /// read stale live content and write it back into the freshly imported DB.
    pub fn sync_to_live(state: &AppState, app: AppType) -> Result<(), AppError> {
        // Pi derives activation from its native AGENTS.md; its persisted prompt
        // rows are intentionally disabled and must not drive generic projection.

        let _guard = PROMPT_FILE_LOCK
            .lock()
            .map_err(|error| AppError::Message(error.to_string()))?;
        let prompts = state.db.get_prompts(&app)?;
        let target_path = prompt_file_path(&app)?;
        if let Some(prompt) = prompts.values().find(|prompt| prompt.enabled) {
            validate_prompt_content(&app, &prompt.content)?;
        }
        if let Some(warning) = project_prompt_set_to_path(&prompts, &target_path)? {
            return Err(AppError::Message(warning));
        }
        Ok(())
    }

    /// Best-effort projection for every Prompt-capable application.
    pub fn sync_all_to_live(state: &AppState) -> Result<(), AppError> {
        let mut failures = Vec::new();
        for app in AppType::all() {
            if let Err(error) = Self::sync_to_live(state, app.clone()) {
                log::warn!("同步 Prompt 到 {app:?} 失败: {error}");
                failures.push(format!("{}: {error}", app.as_str()));
            }
        }

        if failures.is_empty() {
            Ok(())
        } else {
            Err(AppError::Message(format!(
                "部分应用 Prompt 同步失败: {}",
                failures.join("; ")
            )))
        }
    }

    /// 首次启动时从现有提示词文件自动导入（如果存在）
    /// 返回导入的数量
    pub fn import_from_file_on_first_launch(
        state: &AppState,
        app: AppType,
    ) -> Result<usize, AppError> {
        let _guard = PROMPT_FILE_LOCK
            .lock()
            .map_err(|error| AppError::Message(error.to_string()))?;
        // 幂等性保护：该应用已有提示词则跳过
        let existing = state.db.get_prompts(&app)?;
        if !existing.is_empty() {
            return Ok(0);
        }

        let file_path = prompt_file_path(&app)?;

        // 读取文件内容。Pi 与交互式管理路径共用限长读取和协调锁。
        let content = {
            match read_startup_prompt(&file_path)? {
                Some(content) => content,
                None => return Ok(0),
            }
        };

        validate_prompt_content(&app, &content)?;
        // 检查内容是否为空
        if content.trim().is_empty() {
            return Ok(0);
        }

        log::info!("发现提示词文件，自动导入: {file_path:?}");

        // 创建提示词对象
        let timestamp = get_unix_timestamp()?;
        let id = format!("auto-imported-{timestamp}");
        let prompt = Prompt {
            id: id.clone(),
            name: format!(
                "Auto-imported Prompt {}",
                chrono::Local::now().format("%Y-%m-%d %H:%M")
            ),
            content,
            description: Some("Automatically imported on first launch".to_string()),
            // Pi derives active state from AGENTS.md. Other apps retain their
            // established persisted prompt selection.
            enabled: true,
            created_at: Some(timestamp),
            updated_at: Some(timestamp),
        };

        // 保存到数据库
        state.db.save_prompt(&app, &prompt)?;

        log::info!("自动导入完成: {}", app.as_str());
        Ok(1)
    }
}

fn read_startup_prompt(path: &Path) -> Result<Option<String>, AppError> {
    read_text_file(path)
}

#[cfg(test)]
fn upsert_file_prompt(
    state: &AppState,
    app: &AppType,
    id: &str,
    prompt: Prompt,
    target_path: &Path,
) -> Result<(), AppError> {
    upsert_file_prompt_checked(state, app, id, prompt, target_path, None)
}

fn check_prompt_expectation(
    previous: Option<&Prompt>,
    live_content: Option<&str>,
    expected: &PromptWriteExpectation,
) -> Result<(), AppError> {
    if previous != expected.prompt.as_ref()
        || (previous.is_some_and(|prompt| prompt.enabled)
            && live_content != expected.file_content.as_deref())
    {
        return Err(AppError::Conflict(
            "Prompt or native file changed; keep your draft and reload before saving".into(),
        ));
    }
    Ok(())
}

fn upsert_file_prompt_checked(
    state: &AppState,
    app: &AppType,
    id: &str,
    mut prompt: Prompt,
    target_path: &Path,
    expected: Option<&PromptWriteExpectation>,
) -> Result<(), AppError> {
    if id != prompt.id {
        return Err(AppError::InvalidInput(
            "Prompt id does not match requested id".to_string(),
        ));
    }
    let _guard = PROMPT_FILE_LOCK
        .lock()
        .map_err(|error| AppError::Message(error.to_string()))?;
    validate_prompt_content(app, &prompt.content)?;
    let mut prompts = state.db.get_prompts(app)?;
    if let Some(expected) = expected {
        let current = if prompts.get(id).is_some_and(|prompt| prompt.enabled) {
            read_text_file(target_path)?
        } else {
            None
        };
        check_prompt_expectation(prompts.get(id), current.as_deref(), expected)?;
    }
    let clear_live = !prompt.enabled
        && prompts.get(id).is_some_and(|previous| previous.enabled)
        && !prompts
            .iter()
            .any(|(key, prompt)| key != id && prompt.enabled)
        && target_path
            .try_exists()
            .map_err(|error| AppError::io(target_path, error))?;
    if !prompt.enabled && !clear_live {
        return state.db.save_prompt(app, &prompt);
    }
    // Disabling must preserve the last native contents in the inactive template.
    if clear_live {
        if let Some(content) =
            read_text_file(target_path)?.filter(|content| !content.trim().is_empty())
        {
            if prompts
                .get(id)
                .is_some_and(|previous| previous.content == prompt.content)
            {
                prompt.content = content;
            } else {
                backfill_live_prompt(&mut prompts, target_path)?;
                if let Some(previous) = prompts
                    .get(id)
                    .filter(|previous| previous.content != prompt.content)
                {
                    let mut backup = previous.clone();
                    backup.id = format!("backup-{}", uuid::Uuid::new_v4());
                    backup.enabled = false;
                    prompts.insert(backup.id.clone(), backup);
                }
            }
        }
    }
    if prompt.enabled {
        if !prompts.get(id).is_some_and(|previous| previous.enabled) {
            backfill_live_prompt(&mut prompts, target_path)?;
        }
        for previous in prompts.values_mut() {
            previous.enabled = false;
        }
    }
    prompts.insert(id.to_string(), prompt.clone());
    crate::sync_utils::write_and_commit(
        target_path,
        || write_text_file(target_path, if clear_live { "" } else { &prompt.content }),
        || state.db.save_prompts(app, &prompts),
    )
}

fn enable_file_prompt(
    state: &AppState,
    app: &AppType,
    id: &str,
    target_path: &Path,
) -> Result<(), AppError> {
    let _guard = PROMPT_FILE_LOCK
        .lock()
        .map_err(|error| AppError::Message(error.to_string()))?;
    let mut prompts = state.db.get_prompts(app)?;
    let target = prompts
        .get(id)
        .ok_or_else(|| AppError::InvalidInput(format!("提示词 {id} 不存在")))?;
    validate_prompt_content(app, &target.content)?;

    backfill_live_prompt(&mut prompts, target_path)?;
    for (key, prompt) in &mut prompts {
        prompt.enabled = key == id;
    }
    let content = &prompts[id].content;
    validate_prompt_content(app, content)?;
    crate::sync_utils::write_and_commit(
        target_path,
        || write_text_file(target_path, content),
        || state.db.save_prompts(app, &prompts),
    )
}

fn backfill_live_prompt(
    prompts: &mut IndexMap<String, Prompt>,
    target_path: &Path,
) -> Result<(), AppError> {
    let live_content = read_text_file(target_path)?.unwrap_or_default();
    if !live_content.trim().is_empty() {
        let timestamp = get_unix_timestamp()?;
        if let Some(current) = prompts.values_mut().find(|prompt| prompt.enabled) {
            current.content = live_content;
            current.updated_at = Some(timestamp);
        } else if !prompts
            .values()
            .any(|prompt| prompt.content.trim() == live_content.trim())
        {
            let backup_id = format!("backup-{}", uuid::Uuid::new_v4());
            prompts.insert(
                backup_id.clone(),
                Prompt {
                    id: backup_id,
                    name: format!(
                        "原始提示词 {}",
                        chrono::Local::now().format("%Y-%m-%d %H:%M")
                    ),
                    content: live_content,
                    description: Some("自动备份的原始提示词".to_string()),
                    enabled: false,
                    created_at: Some(timestamp),
                    updated_at: Some(timestamp),
                },
            );
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    #[test]
    fn checked_edits_and_disables_preserve_changed_native_content_and_database() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("AGENTS.md");
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        let original = prompt("active", "original", true);
        state.db.save_prompt(&AppType::Codex, &original).unwrap();
        std::fs::write(&path, "external").unwrap();
        let expected = crate::prompt::PromptWriteExpectation {
            prompt: Some(original.clone()),
            file_content: Some("original".into()),
        };
        for enabled in [true, false] {
            let error = super::upsert_file_prompt_checked(
                &state,
                &AppType::Codex,
                "active",
                prompt("active", "draft", enabled),
                &path,
                Some(&expected),
            )
            .unwrap_err();
            assert!(matches!(error, crate::error::AppError::Conflict(_)));
            assert_eq!(
                state.db.get_prompts(&AppType::Codex).unwrap()["active"],
                original
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
        }
        // A second reader may already have backfilled the file into the database.
        state
            .db
            .save_prompt(&AppType::Codex, &prompt("active", "external", true))
            .unwrap();
        assert!(super::upsert_file_prompt_checked(
            &state,
            &AppType::Codex,
            "active",
            prompt("active", "draft", true),
            &path,
            Some(&expected)
        )
        .is_err());
    }

    #[test]
    fn disabling_without_a_snapshot_backs_up_native_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("AGENTS.md");
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        state
            .db
            .save_prompt(&AppType::Codex, &prompt("active", "original", true))
            .unwrap();
        std::fs::write(&path, "external\r\n").unwrap();
        super::upsert_file_prompt(
            &state,
            &AppType::Codex,
            "active",
            prompt("active", "original", false),
            &path,
        )
        .unwrap();
        let saved = state.db.get_prompts(&AppType::Codex).unwrap();
        assert_eq!(saved["active"].content, "external\r\n");
        assert!(!saved["active"].enabled);
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "");
    }
    #[test]
    fn startup_prompt_distinguishes_missing_files_from_read_failures() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("AGENTS.md");
        assert_eq!(super::read_startup_prompt(&path).unwrap(), None);
        std::fs::write(&path, [0xff, 0xfe]).unwrap();
        assert!(super::read_startup_prompt(&path).is_err());
        assert!(super::read_startup_prompt(directory.path()).is_err());
        std::fs::write(&path, "native prompt").unwrap();
        assert_eq!(
            super::read_startup_prompt(&path).unwrap().as_deref(),
            Some("native prompt")
        );
    }

    use super::project_prompt_set_to_path;
    use crate::app_config::AppType;
    use crate::prompt::Prompt;
    use indexmap::IndexMap;
    use tempfile::tempdir;

    #[test]
    fn generic_prompt_activation_rolls_back_file_and_partial_database_changes() {
        let directory = tempdir().unwrap();
        let path = directory.path().join("AGENTS.md");
        let state = crate::store::AppState::new(std::sync::Arc::new(
            crate::database::Database::memory().unwrap(),
        ));
        state
            .db
            .save_prompt(&AppType::Codex, &prompt("a", "old", true))
            .unwrap();
        state
            .db
            .save_prompt(&AppType::Codex, &prompt("z", "new", false))
            .unwrap();
        std::fs::write(&path, "external edit").unwrap();
        state.db.conn.lock().unwrap().execute_batch(
            "CREATE TRIGGER fail_prompt BEFORE INSERT ON prompts WHEN NEW.id = 'z' BEGIN SELECT RAISE(ABORT, 'injected'); END;"
        ).unwrap();
        assert!(super::enable_file_prompt(&state, &AppType::Codex, "z", &path).is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "external edit");
        let prompts = state.db.get_prompts(&AppType::Codex).unwrap();
        assert!(prompts["a"].enabled);
        assert!(!prompts["z"].enabled);
        assert_eq!(prompts["a"].content, "old");
    }

    fn prompt(id: &str, content: &str, enabled: bool) -> Prompt {
        Prompt {
            id: id.to_string(),
            name: id.to_string(),
            content: content.to_string(),
            description: None,
            enabled,
            created_at: None,
            updated_at: None,
        }
    }

    #[test]
    fn enabled_upsert_does_not_publish_backup_or_activation_on_database_failure() {
        use super::upsert_file_prompt;
        use crate::{database::Database, store::AppState};
        use std::sync::Arc;

        let temp = tempdir().unwrap();
        let path = temp.path().join("AGENTS.md");
        std::fs::write(&path, "unmanaged").unwrap();
        let state = AppState::new(Arc::new(Database::memory().unwrap()));
        state
            .db
            .conn
            .lock()
            .unwrap()
            .execute_batch("PRAGMA query_only = ON")
            .unwrap();
        assert!(upsert_file_prompt(
            &state,
            &AppType::Claude,
            "new",
            prompt("new", "new body", true),
            &path
        )
        .is_err());
        assert_eq!(std::fs::read_to_string(&path).unwrap(), "unmanaged");
        assert!(state.db.get_prompts(&AppType::Claude).unwrap().is_empty());
    }

    #[test]
    fn restored_prompt_projection_writes_the_enabled_content() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("AGENTS.md");
        let mut prompts = IndexMap::new();
        prompts.insert("off".to_string(), prompt("off", "old", false));
        prompts.insert("on".to_string(), prompt("on", "restored", true));

        let warning = project_prompt_set_to_path(&prompts, &path).expect("project prompt");
        assert!(warning.is_none());
        assert_eq!(
            std::fs::read_to_string(path).expect("read prompt"),
            "restored"
        );
    }

    #[test]
    fn restored_prompt_projection_preserves_the_live_file_when_none_are_enabled() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("AGENTS.md");
        std::fs::write(&path, "local content").expect("seed live prompt file");
        let mut prompts = IndexMap::new();
        prompts.insert("off".to_string(), prompt("off", "managed", false));

        let warning = project_prompt_set_to_path(&prompts, &path).expect("project prompt");
        assert!(warning.is_none());
        // The live file is not part of the sync payload, so a restore with no
        // enabled prompt must not wipe local content it never contained.
        assert_eq!(
            std::fs::read_to_string(path).expect("read prompt"),
            "local content"
        );
    }

    #[test]
    fn restored_prompt_projection_selects_the_first_enabled_prompt_deterministically() {
        let temp = tempdir().expect("tempdir");
        let path = temp.path().join("AGENTS.md");
        let mut prompts = IndexMap::new();
        prompts.insert("first".to_string(), prompt("first", "first body", true));
        prompts.insert("second".to_string(), prompt("second", "second body", true));

        let warning = project_prompt_set_to_path(&prompts, &path)
            .expect("project prompt")
            .expect("duplicate enabled prompts should warn");
        assert!(warning.contains("first, second"));
        assert_eq!(
            std::fs::read_to_string(path).expect("read prompt"),
            "first body"
        );
    }
}
