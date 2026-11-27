//! Sanitize custom icons in imported databases without dropping unknown JSON fields.
use super::CUSTOM_ICONS_KEY;
use crate::{
    error::AppError,
    icon::{is_custom_icon, is_valid_custom_icon},
};
use rusqlite::{params, Connection, OptionalExtension};

/// SQL 导入、云同步在替换主库前调用：把暂存库里不合格的自定义图标清空（退回首字母），
/// 其余数据原样保留，不让一个坏图标挡住整份备份。返回清掉的个数。
///
/// 图标存在三处：`providers.icon` 列、settings 表 `universal_providers` 键下的 JSON
/// （统一供应商，同步到应用时会原样复制给子供应商），以及 `custom_icons` 键下的图标库
/// （选中后原样写进供应商）。三处都要清。
pub fn strip_invalid_custom_icons(conn: &Connection) -> Result<usize, AppError> {
    let db_err = |e: rusqlite::Error| AppError::Database(e.to_string());

    let rows: Vec<(String, String, String)> = {
        let mut stmt = conn
            .prepare("SELECT id, app_type, icon FROM providers WHERE icon LIKE 'data:%'")
            .map_err(db_err)?;
        let rows = stmt
            .query_map([], |row| Ok((row.get(0)?, row.get(1)?, row.get(2)?)))
            .map_err(db_err)?
            .collect::<Result<_, _>>()
            .map_err(db_err)?;
        rows
    };
    let mut stripped = 0;
    for (id, app_type, icon) in rows {
        if is_valid_custom_icon(&icon) {
            continue;
        }
        conn.execute(
            "UPDATE providers SET icon = NULL, icon_color = NULL WHERE id = ?1 AND app_type = ?2",
            params![id, app_type],
        )
        .map_err(db_err)?;
        stripped += 1;
    }

    stripped += strip_invalid_universal_icons(conn)?;
    stripped += strip_invalid_library_icons(conn)?;

    if stripped > 0 {
        log::warn!("导入的数据里有 {stripped} 个无效的自定义图标，已清空");
    }
    Ok(stripped)
}

/// 按 JSON 原样改写，不经过 `UniversalProvider` 结构体：较新版本写进来的未知字段要保留。
/// JSON 解析不了就不动，交给读取它的地方报错。
fn strip_invalid_universal_icons(conn: &Connection) -> Result<usize, AppError> {
    let db_err = |e: rusqlite::Error| AppError::Database(e.to_string());
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = 'universal_providers'",
            [],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    let Some(raw) = raw else {
        return Ok(0);
    };
    let Ok(mut value) = serde_json::from_str::<serde_json::Value>(&raw) else {
        return Ok(0);
    };
    let Some(providers) = value.as_object_mut() else {
        return Ok(0);
    };
    let mut stripped = 0;
    for provider in providers.values_mut() {
        let Some(provider) = provider.as_object_mut() else {
            continue;
        };
        let invalid = provider
            .get("icon")
            .and_then(|icon| icon.as_str())
            .is_some_and(|icon| is_custom_icon(icon) && !is_valid_custom_icon(icon));
        if invalid {
            provider.remove("icon");
            provider.remove("iconColor");
            stripped += 1;
        }
    }
    if stripped > 0 {
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = 'universal_providers'",
            params![value.to_string()],
        )
        .map_err(db_err)?;
    }
    Ok(stripped)
}

/// 图标库里 `data` 不是合格自定义图标的条目整条删掉。和统一供应商一样按 JSON 改写，
/// 保留其余条目的未知字段；数组解析不了就不动（读取时按空库处理）。
fn strip_invalid_library_icons(conn: &Connection) -> Result<usize, AppError> {
    let db_err = |e: rusqlite::Error| AppError::Database(e.to_string());
    let raw: Option<String> = conn
        .query_row(
            "SELECT value FROM settings WHERE key = ?1",
            [CUSTOM_ICONS_KEY],
            |row| row.get(0),
        )
        .optional()
        .map_err(db_err)?;
    let Some(raw) = raw else {
        return Ok(0);
    };
    let Ok(mut entries) = serde_json::from_str::<Vec<serde_json::Value>>(&raw) else {
        return Ok(0);
    };
    let before = entries.len();
    entries.retain(|entry| {
        entry
            .get("data")
            .and_then(|data| data.as_str())
            .is_some_and(is_valid_custom_icon)
    });
    let stripped = before - entries.len();
    if stripped > 0 {
        conn.execute(
            "UPDATE settings SET value = ?1 WHERE key = ?2",
            params![
                serde_json::Value::Array(entries).to_string(),
                CUSTOM_ICONS_KEY
            ],
        )
        .map_err(db_err)?;
    }
    Ok(stripped)
}
