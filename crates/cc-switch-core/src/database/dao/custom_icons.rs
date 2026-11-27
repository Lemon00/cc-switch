//! 「我的图标」：用户上传过的自定义图标库
//!
//! 存在 settings 表 `custom_icons` 键下，是一个 JSON 数组，随云同步。图标库只是挑选用的
//! 素材：供应商选中某个图标时，`providers.icon` 存的是图片本身（data URI），不是这里的 id，
//! 所以删掉或改名图标库里的条目，不影响已经在用它的供应商。

use rusqlite::OptionalExtension;
use serde::{Deserialize, Serialize};

use crate::database::{lock_conn, to_json_string, Database};
use crate::error::AppError;

/// 图标库的 settings 键，导入清理（`services/provider/icon.rs`）也按这个键找
pub const CUSTOM_ICONS_KEY: &str = "custom_icons";

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomIconEntry {
    pub id: String,
    pub name: String,
    /// `data:image/png;base64,…`，和 `providers.icon` 里的自定义图标同一种格式
    pub data: String,
    #[serde(default)]
    pub created_at: i64,
    /// 较新版本写进来的字段，读进来再原样写回去
    #[serde(flatten)]
    pub extra: serde_json::Map<String, serde_json::Value>,
}

impl Database {
    /// Read/modify/write under one connection transaction; callers must not reenter the DB.
    pub(crate) fn mutate_custom_icons<T>(
        &self,
        mutate: impl FnOnce(&mut Vec<CustomIconEntry>) -> Result<T, AppError>,
    ) -> Result<T, AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;
        let raw: Option<String> = tx
            .query_row(
                "SELECT value FROM settings WHERE key = ?1",
                [CUSTOM_ICONS_KEY],
                |row| row.get(0),
            )
            .optional()
            .map_err(|e| AppError::Database(e.to_string()))?;
        // A display read may skip malformed entries. A read/modify/write must
        // reject them, otherwise any edit silently deletes their stored bytes.
        let mut icons: Vec<CustomIconEntry> = match raw.as_deref() {
            Some(raw) => serde_json::from_str(raw).map_err(|error| {
                AppError::Database(format!("图标库包含无法解析的条目，已保留原数据: {error}"))
            })?,
            None => Vec::new(),
        };
        let result = mutate(&mut icons)?;
        let json = to_json_string(&icons)?;
        if raw.as_deref() != Some(json.as_str()) {
            tx.execute(
                "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
                [CUSTOM_ICONS_KEY, &json],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        }
        tx.commit().map_err(|e| AppError::Database(e.to_string()))?;
        Ok(result)
    }

    /// 读出图标库，按数组顺序（新上传的在前）。
    ///
    /// 根节点不是合法 JSON 数组时失败关闭；单条坏条目跳过并记日志。
    pub fn get_custom_icons(&self) -> Result<Vec<CustomIconEntry>, AppError> {
        parse_custom_icons(self.get_setting(CUSTOM_ICONS_KEY)?.as_deref())
    }

    pub fn save_custom_icons(&self, icons: &[CustomIconEntry]) -> Result<(), AppError> {
        let json = to_json_string(&icons)?;
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, ?2)",
            [CUSTOM_ICONS_KEY, &json],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }
}

fn parse_custom_icons(raw: Option<&str>) -> Result<Vec<CustomIconEntry>, AppError> {
    let Some(raw) = raw else {
        return Ok(Vec::new());
    };
    let value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| AppError::Database(format!("图标库数据无法解析: {error}")))?;
    let values = value
        .as_array()
        .ok_or_else(|| AppError::Database("图标库根节点必须是 JSON 数组".to_string()))?;
    let mut icons = Vec::with_capacity(values.len());
    for entry in values {
        match serde_json::from_value::<CustomIconEntry>(entry.clone()) {
            Ok(icon) => icons.push(icon),
            Err(error) => log::warn!("跳过无法解析的图标库条目: {error}"),
        }
    }
    Ok(icons)
}
