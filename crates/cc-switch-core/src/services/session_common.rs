//! 会话同步共享 helper（不 gate）：Codex 会话同步与迁移依赖它们，
//! 因此即使关掉 `session-monitor` 也必须在。聚合层（session_usage.rs
//! 及其 per-app 模块）通过重导出继续用这些名字。

use crate::database::{lock_conn, Database};
use crate::error::AppError;
use serde::{Deserialize, Serialize};
use std::fs;
use std::time::SystemTime;

/// 同步结果
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionSyncResult {
    pub imported: u32,
    pub skipped: u32,
    pub files_scanned: u32,
    pub suspected_duplicates: u32,
    pub deferred_files: u32,
    pub errors: Vec<String>,
}

impl SessionSyncResult {
    pub fn merge(&mut self, other: SessionSyncResult) {
        self.imported = self.imported.saturating_add(other.imported);
        self.skipped = self.skipped.saturating_add(other.skipped);
        self.files_scanned = self.files_scanned.saturating_add(other.files_scanned);
        self.suspected_duplicates = self
            .suspected_duplicates
            .saturating_add(other.suspected_duplicates);
        self.deferred_files = self.deferred_files.saturating_add(other.deferred_files);
        self.errors.extend(other.errors);
    }
}

const MIN_PLAUSIBLE_LATENCY_MS: i64 = 100;
const MAX_PLAUSIBLE_LATENCY_MS: i64 = 60 * 60 * 1000;

pub fn estimated_latency_ms(start_ms: i64, end_ms: i64) -> Option<i64> {
    let latency = end_ms.checked_sub(start_ms)?;
    (MIN_PLAUSIBLE_LATENCY_MS..=MAX_PLAUSIBLE_LATENCY_MS)
        .contains(&latency)
        .then_some(latency)
}

pub fn parse_timestamp_millis(timestamp: &str) -> Option<i64> {
    chrono::DateTime::parse_from_rfc3339(timestamp)
        .ok()
        .map(|dt| dt.timestamp_millis())
}

pub fn estimated_first_token_ms(
    start_ms: i64,
    first_output_ms: i64,
    latency_ms: i64,
) -> Option<i64> {
    let first_token = first_output_ms.checked_sub(start_ms)?;
    (first_token > 0 && first_token <= latency_ms).then_some(first_token)
}

pub fn metadata_modified_nanos(metadata: &fs::Metadata) -> i64 {
    metadata
        .modified()
        .ok()
        .and_then(|t| t.duration_since(SystemTime::UNIX_EPOCH).ok())
        .map(|d| d.as_nanos().min(i64::MAX as u128) as i64)
        .unwrap_or(0)
}

pub fn update_sync_state(
    db: &Database,
    file_path: &str,
    last_modified: i64,
    last_offset: i64,
) -> Result<(), AppError> {
    let conn = lock_conn!(db.conn);
    update_sync_state_on_conn(&conn, file_path, last_modified, last_offset)
}

pub fn update_sync_state_on_conn(
    conn: &rusqlite::Connection,
    file_path: &str,
    last_modified: i64,
    last_offset: i64,
) -> Result<(), AppError> {
    let now = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);

    conn.prepare_cached(
        "INSERT OR REPLACE INTO session_log_sync (file_path, last_modified, last_line_offset, last_synced_at)
         VALUES (?1, ?2, ?3, ?4)",
    )
    .and_then(|mut stmt| stmt.execute(rusqlite::params![file_path, last_modified, last_offset, now]))
    .map_err(|e| AppError::Database(format!("更新同步状态失败: {e}")))?;
    Ok(())
}

/// 按日志时间戳估出来的计时：耗时（含首字等待）和首字时间（推不出来为 None）。
/// Claude 和 Codex 的会话日志导入共用。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct EstimatedTiming {
    pub latency_ms: i64,
    pub first_token_ms: Option<i64>,
}
