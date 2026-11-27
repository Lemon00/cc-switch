//! Compensating deletion of native files and the SQLite index. Staged files stay recoverable
//! when an I/O failure prevents rollback; cleanup never owns the only copy implicitly.
use super::{
    jetbox_without, trajectory_cache_key, valid_session_id, wal_sibling_path, TRAJECTORY_CACHE,
};
use std::{
    fs,
    io::Read,
    path::{Path, PathBuf},
    time::Duration,
};

pub(super) fn delete(root: &Path, path: &Path, session_id: &str) -> Result<bool, String> {
    if !valid_session_id(session_id) {
        return Err("Invalid Antigravity session ID".into());
    }
    let expected = root
        .join(session_id)
        .join(".system_generated/logs/transcript_full.jsonl");
    if path != expected {
        return Err(format!(
            "Unexpected Antigravity session source: {}",
            path.display()
        ));
    }
    let app = root
        .parent()
        .filter(|_| root.file_name().is_some_and(|name| name == "brain"))
        .filter(|dir| {
            dir.file_name()
                .is_some_and(|name| name == "antigravity-cli")
        })
        .ok_or("Delete this session in Antigravity to keep its index and summaries consistent")?;
    let app = app.canonicalize().map_err(|e| e.to_string())?;
    let presence = app.join("presence").join(format!("{session_id}.lock"));
    let jetbox = app.join("jetbox_summaries_proto.pb");
    let summary = app.join("conversation_summaries.db");
    let conversation = app.join("conversations").join(format!("{session_id}.db"));
    let targets = [
        (conversation.clone(), false),
        (wal_sibling_path(&conversation), false),
        (conversation.with_extension("db-shm"), false),
        (conversation.with_extension("db-journal"), false),
        (
            app.join("annotations").join(format!("{session_id}.pbtxt")),
            false,
        ),
        (app.join("brain").join(session_id), true),
    ];
    // Validating the transcript alone does not protect its sibling indexes and stores.
    for (target, directory) in &targets {
        checked_path(&app, target, *directory)?;
    }
    checked_path(&app, &presence, false)?;
    checked_path(&app, &jetbox, false)?;
    let summary_exists = checked_path(&app, &summary, false)?;
    for suffix in ["db-wal", "db-shm", "db-journal"] {
        checked_path(&app, &summary.with_extension(suffix), false)?;
    }
    fs::create_dir_all(presence.parent().unwrap()).map_err(|e| e.to_string())?;
    let _presence_lock = lock_presence(&presence)?;
    let cache_key = trajectory_cache_key(&conversation);
    let mut connection = if summary_exists {
        let conn = rusqlite::Connection::open_with_flags(
            &summary,
            rusqlite::OpenFlags::SQLITE_OPEN_READ_WRITE | rusqlite::OpenFlags::SQLITE_OPEN_NO_MUTEX,
        )
        .map_err(|e| format!("Cannot open Antigravity conversation list: {e}"))?;
        conn.busy_timeout(Duration::from_secs(5))
            .map_err(|e| e.to_string())?;
        Some(conn)
    } else {
        None
    };
    let transaction = connection
        .as_mut()
        .map(|conn| conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate))
        .transpose()
        .map_err(|e| format!("Cannot lock Antigravity conversation list: {e}"))?;
    let original = read_index(&jetbox)?;
    let replacement = original
        .as_deref()
        .map(|bytes| jetbox_without(bytes, session_id))
        .transpose()?
        .flatten();
    let stage = tempfile::Builder::new()
        .prefix(".cc-switch-agy-delete-")
        .tempdir_in(&app)
        .map_err(|e| format!("Cannot stage Antigravity deletion: {e}"))?
        .keep();
    let mut deletion = StagedDeletion {
        stage,
        moved: Vec::new(),
        jetbox,
        original,
        replacement,
        index_attempted: false,
        finished: false,
    };
    let result: Result<(), String> = (|| {
        if let Some(tx) = transaction.as_ref() {
            delete_summary_rows(tx, session_id)?;
        }
        for (target, directory) in &targets {
            if !checked_path(&app, target, *directory)? {
                continue;
            }
            let destination = deletion.stage.join(deletion.moved.len().to_string());
            fs::rename(target, &destination)
                .map_err(|e| format!("Cannot stage {}: {e}", target.display()))?;
            deletion.moved.push((target.clone(), destination));
        }
        if let Some(bytes) = deletion.replacement.as_ref() {
            if read_index(&deletion.jetbox)? != deletion.original {
                return Err("Antigravity conversation index changed; retry deletion".into());
            }
            deletion.index_attempted = true;
            crate::config::atomic_write_private(&deletion.jetbox, bytes)
                .map_err(|e| format!("Cannot update Antigravity conversation index: {e}"))?;
        }
        if let Some(tx) = transaction {
            tx.commit().map_err(|e| e.to_string())?;
        }
        Ok(())
    })();
    if let Err(error) = result {
        return match deletion.rollback() {
            Ok(()) => Err(error),
            Err(rollback) => Err(format!("{error}; {rollback}")),
        };
    }
    deletion.finished = true;
    TRAJECTORY_CACHE
        .lock()
        .unwrap_or_else(|poison| poison.into_inner())
        .remove(&cache_key);
    if let Err(error) = fs::remove_dir_all(&deletion.stage) {
        log::warn!(
            "Deleted Antigravity session; cleanup retained at {}: {error}",
            deletion.stage.display()
        );
    }
    // Keep the presence inode: unlinking after unlock lets another process acquire an orphaned
    // lock while a third process creates a new lock at the same pathname.
    Ok(true)
}

fn checked_path(base: &Path, path: &Path, directory: bool) -> Result<bool, String> {
    let relative = path
        .strip_prefix(base)
        .map_err(|_| "Antigravity path escapes its store")?;
    let mut current = base.to_path_buf();
    for component in relative.components() {
        if !matches!(component, std::path::Component::Normal(_)) {
            return Err("Invalid Antigravity store path".into());
        }
        current.push(component);
        let meta = match fs::symlink_metadata(&current) {
            Ok(meta) => meta,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(false),
            Err(e) => return Err(format!("Cannot inspect {}: {e}", current.display())),
        };
        #[cfg(windows)]
        let linked = {
            use std::os::windows::fs::MetadataExt;
            meta.file_attributes() & 0x400 != 0 // includes junctions and other reparse points
        };
        #[cfg(not(windows))]
        let linked = meta.file_type().is_symlink();
        let want_dir = current != path || directory;
        if linked || (want_dir && !meta.is_dir()) || (!want_dir && !meta.is_file()) {
            return Err(format!(
                "Unsafe Antigravity store path: {}",
                current.display()
            ));
        }
    }
    Ok(true)
}

#[allow(clippy::incompatible_msrv)]
fn lock_presence(path: &Path) -> Result<fs::File, String> {
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .map_err(|e| format!("Cannot check whether agy is using this session: {e}"))?;
    match file.try_lock() {
        Ok(()) => Ok(file),
        Err(fs::TryLockError::WouldBlock) => {
            Err("This conversation is open in agy. Close it there before deleting".into())
        }
        Err(fs::TryLockError::Error(e)) => Err(format!(
            "Cannot check whether agy is using this session: {e}"
        )),
    }
}

fn read_index(path: &Path) -> Result<Option<Vec<u8>>, String> {
    const MAX_BYTES: u64 = 32 * 1024 * 1024;
    let file = match fs::File::open(path) {
        Ok(file) => file,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("Cannot read Antigravity conversation index: {e}")),
    };
    let mut bytes = Vec::new();
    file.take(MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| e.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("Antigravity conversation index exceeds 32 MB".into());
    }
    Ok(Some(bytes))
}

fn delete_summary_rows(tx: &rusqlite::Transaction<'_>, id: &str) -> Result<(), String> {
    for table in [
        "conversation_summaries",
        "conversations_fts",
        "conversation_parents",
    ] {
        let exists: bool = tx
            .query_row(
                "SELECT EXISTS(SELECT 1 FROM sqlite_master WHERE name = ?1)",
                [table],
                |row| row.get(0),
            )
            .map_err(|e| e.to_string())?;
        if exists {
            tx.execute(
                &format!("DELETE FROM {table} WHERE conversation_id = ?1"),
                [id],
            )
            .map_err(|e| format!("Cannot update Antigravity conversation list: {e}"))?;
        }
    }
    Ok(())
}

struct StagedDeletion {
    stage: PathBuf,
    moved: Vec<(PathBuf, PathBuf)>,
    jetbox: PathBuf,
    original: Option<Vec<u8>>,
    replacement: Option<Vec<u8>>,
    index_attempted: bool,
    finished: bool,
}

impl StagedDeletion {
    fn rollback(&mut self) -> Result<(), String> {
        let mut errors = Vec::new();
        if self.index_attempted {
            let restored = (|| {
                let current = read_index(&self.jetbox)?;
                if current == self.original {
                    return Ok(());
                }
                if current != self.replacement && current.is_some() {
                    return Err("Antigravity index changed during rollback".to_string());
                }
                if let Some(bytes) = &self.original {
                    crate::config::atomic_write_private(&self.jetbox, bytes)
                        .map_err(|e| e.to_string())?;
                }
                Ok(())
            })();
            if let Err(error) = restored {
                errors.push(error);
            }
        }
        for (original, staged) in self.moved.iter().rev() {
            match fs::symlink_metadata(original) {
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                    if let Err(e) = fs::rename(staged, original) {
                        errors.push(format!("Cannot restore {}: {e}", original.display()));
                    }
                }
                _ => errors.push(format!("Restore target occupied: {}", original.display())),
            }
        }
        self.finished = true;
        if errors.is_empty() {
            let _ = fs::remove_dir(&self.stage);
            Ok(())
        } else {
            // Preserve the index snapshot alongside staged files if rollback itself fails.
            if let Some(bytes) = &self.original {
                let _ = crate::config::atomic_write_private(
                    &self.stage.join("jetbox-original.pb"),
                    bytes,
                );
            }
            Err(format!(
                "Recovery files retained at {}: {}",
                self.stage.display(),
                errors.join("; ")
            ))
        }
    }
}

impl Drop for StagedDeletion {
    fn drop(&mut self) {
        if !self.finished {
            if let Err(error) = self.rollback() {
                log::error!("{error}");
            }
        }
    }
}
