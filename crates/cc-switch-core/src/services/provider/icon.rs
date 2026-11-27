//! 供应商的自定义图标。
//!
//! `providers.icon` 原本只存内置图标名，用户上传的图片也存在这一列：前端把图片缩成
//! 小尺寸 PNG，以 `data:image/png;base64,…` 的形式写进来，随行数据一起同步、复制。
//! 原始文件（SVG、带 EXIF 的 JPEG 等）不进库。保存时用 [`validate_icon`] 完整解码一遍；
//! SQL 导入和云同步不经过保存，由 [`strip_invalid_custom_icons`] 在暂存库里清理
//! （`providers` 表、settings 里的统一供应商 JSON 和「我的图标」库都要清）。
//! 「我的图标」是用户上传过的图片，存在 settings 的 `custom_icons` 键下，供选择器复用。
//! 前端渲染时另有一道同样的长度、宽高检查兜底（`src/utils/customIcon.ts`）。

use crate::database::{CustomIconEntry, Database};
use crate::error::AppError;

pub use crate::database::icon_sanitizer::strip_invalid_custom_icons;
use crate::icon::is_valid_custom_icon;
pub use crate::icon::{is_custom_icon, validate_icon, CUSTOM_ICON_PREFIX, MAX_CUSTOM_ICON_LENGTH};

// --- 我的图标（图标库） ---

/// 图标库条目数上限：每张 96×96 的 PNG 一般几 KB，最坏不到 64 KiB，整个库都存在一个
/// settings 值里，打开选择器时一次读出
pub const MAX_LIBRARY_ICONS: usize = 100;

/// 名称长度上限（按字符数）
pub const MAX_LIBRARY_NAME_CHARS: usize = 64;

/// [`add_library_icon`] 的结果：同一张图已经在库里时返回原条目，`existed` 为 true
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AddLibraryIconResult {
    pub icon: CustomIconEntry,
    pub existed: bool,
}

fn normalize_library_name(name: &str) -> Result<String, AppError> {
    let name = name.trim();
    if name.is_empty() {
        return Err(AppError::localized(
            "customIcon.nameEmpty",
            "请给图标起个名字",
            "Please name the icon",
        ));
    }
    if name.chars().count() > MAX_LIBRARY_NAME_CHARS {
        return Err(AppError::localized(
            "customIcon.nameTooLong",
            format!("图标名称不能超过 {MAX_LIBRARY_NAME_CHARS} 个字符"),
            format!("The icon name cannot exceed {MAX_LIBRARY_NAME_CHARS} characters"),
        ));
    }
    Ok(name.to_string())
}

fn library_icon_not_found() -> AppError {
    AppError::localized(
        "customIcon.notFound",
        "这个图标已经不在「我的图标」里了",
        "This icon is no longer in My Icons",
    )
}

pub fn list_library_icons(db: &Database) -> Result<Vec<CustomIconEntry>, AppError> {
    db.get_custom_icons()
}

/// 存进图标库，新的排在最前面。图片和供应商保存时一样完整校验；同一张图已经在库里时
/// 不重复存，返回原条目（名称不改）。
pub fn add_library_icon(
    db: &Database,
    name: &str,
    data: &str,
) -> Result<AddLibraryIconResult, AppError> {
    let name = normalize_library_name(name)?;
    if !is_valid_custom_icon(data) {
        return Err(AppError::localized(
            "provider.icon.invalid",
            "自定义图标无效，请重新上传",
            "The custom icon is invalid, please upload it again",
        ));
    }
    db.mutate_custom_icons(|icons| {
        if let Some(existing) = icons.iter().find(|icon| icon.data == data) {
            return Ok(AddLibraryIconResult {
                icon: existing.clone(),
                existed: true,
            });
        }
        if icons.len() >= MAX_LIBRARY_ICONS {
            return Err(AppError::localized(
                "customIcon.libraryFull",
                format!("「我的图标」最多存 {MAX_LIBRARY_ICONS} 个，请先删掉一些"),
                format!("My Icons holds up to {MAX_LIBRARY_ICONS} icons, please delete some first"),
            ));
        }
        let icon = CustomIconEntry {
            id: uuid::Uuid::new_v4().to_string(),
            name,
            data: data.to_string(),
            created_at: chrono::Utc::now().timestamp_millis(),
            extra: serde_json::Map::new(),
        };
        icons.insert(0, icon.clone());
        Ok(AddLibraryIconResult {
            icon,
            existed: false,
        })
    })
}

pub fn rename_library_icon(
    db: &Database,
    id: &str,
    name: &str,
) -> Result<CustomIconEntry, AppError> {
    let name = normalize_library_name(name)?;
    db.mutate_custom_icons(|icons| {
        let icon = icons
            .iter_mut()
            .find(|icon| icon.id == id)
            .ok_or_else(library_icon_not_found)?;
        icon.name = name;
        let renamed = icon.clone();
        Ok(renamed)
    })
}

/// 只删图标库里的条目，已经选了这张图的供应商自己存着图片，不受影响。
/// 条目已经不在（比如另一台设备删了又同步过来）时什么都不做。
pub fn delete_library_icon(db: &Database, id: &str) -> Result<(), AppError> {
    db.mutate_custom_icons(|icons| {
        icons.retain(|icon| icon.id != id);
        Ok(())
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::database::CUSTOM_ICONS_KEY;
    use base64::Engine;
    use rusqlite::{params, Connection};

    /// 用真正的编码器生成一张 PNG
    fn encode_png(width: u32, height: u32) -> Vec<u8> {
        let mut out = Vec::new();
        {
            let mut encoder = png::Encoder::new(&mut out, width, height);
            encoder.set_color(png::ColorType::Rgba);
            encoder.set_depth(png::BitDepth::Eight);
            let mut writer = encoder.write_header().unwrap();
            let pixels: Vec<u8> = (0..width * height * 4).map(|i| (i % 251) as u8).collect();
            writer.write_image_data(&pixels).unwrap();
        }
        out
    }

    fn data_uri(bytes: &[u8]) -> String {
        format!(
            "{CUSTOM_ICON_PREFIX}{}",
            base64::engine::general_purpose::STANDARD.encode(bytes)
        )
    }

    #[test]
    fn concurrent_library_uploads_preserve_every_icon() {
        let db = Database::memory().unwrap();
        let barrier = std::sync::Barrier::new(8);
        std::thread::scope(|scope| {
            for size in 1..=8 {
                let db = &db;
                let barrier = &barrier;
                scope.spawn(move || {
                    let data = data_uri(&encode_png(size, size));
                    barrier.wait();
                    add_library_icon(db, &format!("Icon {size}"), &data).unwrap();
                });
            }
        });
        assert_eq!(db.get_custom_icons().unwrap().len(), 8);
    }

    #[test]
    fn concurrent_identical_uploads_share_one_entry() {
        let db = Database::memory().unwrap();
        let data = data_uri(&encode_png(2, 2));
        let barrier = std::sync::Barrier::new(8);
        let added = std::sync::atomic::AtomicUsize::new(0);
        std::thread::scope(|scope| {
            for _ in 0..8 {
                scope.spawn(|| {
                    barrier.wait();
                    if !add_library_icon(&db, "Icon", &data).unwrap().existed {
                        added.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                    }
                });
            }
        });
        assert_eq!(added.load(std::sync::atomic::Ordering::SeqCst), 1);
        assert_eq!(db.get_custom_icons().unwrap().len(), 1);
    }

    #[test]
    fn accepts_builtin_names_and_empty() {
        assert!(validate_icon(None).is_ok());
        assert!(validate_icon(Some("claude")).is_ok());
        // 库里可能有旧版深链接写进来的任意字符串，编辑时不能因此保存失败
        assert!(validate_icon(Some("https://example.com/logo.png")).is_ok());
    }

    #[test]
    fn accepts_a_real_small_png() {
        assert!(validate_icon(Some(&data_uri(&encode_png(96, 96)))).is_ok());
    }

    #[test]
    fn rejects_truncated_or_header_only_png() {
        let png = encode_png(96, 96);
        // 只有签名 + IHDR，没有像素数据
        assert!(validate_icon(Some(&data_uri(&png[..33]))).is_err());
        // 像素数据截了一半
        assert!(validate_icon(Some(&data_uri(&png[..png.len() / 2]))).is_err());
        // 缺 IEND
        assert!(validate_icon(Some(&data_uri(&png[..png.len() - 12]))).is_err());
        // 改坏一个字节，块校验和对不上
        let mut corrupted = png.clone();
        let mid = corrupted.len() / 2;
        corrupted[mid] ^= 0xff;
        assert!(validate_icon(Some(&data_uri(&corrupted))).is_err());
    }

    #[test]
    fn rejects_other_data_uris() {
        let svg = format!(
            "data:image/svg+xml;base64,{}",
            base64::engine::general_purpose::STANDARD.encode("<svg onload=\"alert(1)\"/>")
        );
        assert!(validate_icon(Some(&svg)).is_err());
        assert!(validate_icon(Some("data:image/png;base64,!!!not-base64")).is_err());
        assert!(validate_icon(Some(&data_uri(b"GIF89a-padding-padding-padding"))).is_err());
    }

    #[test]
    fn rejects_oversized_png() {
        assert!(validate_icon(Some(&data_uri(&encode_png(300, 10)))).is_err());

        let mut big = encode_png(96, 96);
        big.resize(MAX_CUSTOM_ICON_LENGTH, 0);
        assert!(validate_icon(Some(&data_uri(&big))).is_err());
    }

    #[test]
    fn strips_only_invalid_icons_from_imported_rows() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE providers (id TEXT, app_type TEXT, icon TEXT, icon_color TEXT);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        let good = data_uri(&encode_png(96, 96));
        let too_long = format!("{CUSTOM_ICON_PREFIX}{}", "A".repeat(MAX_CUSTOM_ICON_LENGTH));
        for (id, icon) in [
            ("good", good.as_str()),
            ("long", too_long.as_str()),
            ("builtin", "claude"),
        ] {
            conn.execute(
                "INSERT INTO providers VALUES (?1, 'claude', ?2, '#fff')",
                params![id, icon],
            )
            .unwrap();
        }

        assert_eq!(strip_invalid_custom_icons(&conn).unwrap(), 1);

        let icon_of = |id: &str| -> Option<String> {
            conn.query_row(
                "SELECT icon FROM providers WHERE id = ?1",
                params![id],
                |row| row.get(0),
            )
            .unwrap()
        };
        assert_eq!(icon_of("good"), Some(good.clone()));
        assert_eq!(icon_of("long"), None);
        assert_eq!(icon_of("builtin"), Some("claude".to_string()));
    }

    #[test]
    fn strips_invalid_icons_from_universal_providers_and_keeps_other_fields() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE providers (id TEXT, app_type TEXT, icon TEXT, icon_color TEXT);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        let good = data_uri(&encode_png(96, 96));
        let header_only = data_uri(&encode_png(96, 96)[..33]);
        let universal = serde_json::json!({
            "u1": { "id": "u1", "name": "Bad", "icon": header_only, "iconColor": "#fff", "futureField": 1 },
            "u2": { "id": "u2", "name": "Good", "icon": good },
            "u3": { "id": "u3", "name": "Builtin", "icon": "claude" },
        });
        conn.execute(
            "INSERT INTO settings VALUES ('universal_providers', ?1)",
            params![universal.to_string()],
        )
        .unwrap();

        assert_eq!(strip_invalid_custom_icons(&conn).unwrap(), 1);

        let raw: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'universal_providers'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let after: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert!(after["u1"].get("icon").is_none());
        assert!(after["u1"].get("iconColor").is_none());
        assert_eq!(after["u1"]["futureField"], 1);
        assert_eq!(after["u2"]["icon"], good);
        assert_eq!(after["u3"]["icon"], "claude");
    }

    #[test]
    fn strips_invalid_entries_from_icon_library_and_keeps_others() {
        let conn = Connection::open_in_memory().unwrap();
        conn.execute_batch(
            "CREATE TABLE providers (id TEXT, app_type TEXT, icon TEXT, icon_color TEXT);
             CREATE TABLE settings (key TEXT PRIMARY KEY, value TEXT);",
        )
        .unwrap();
        let good = data_uri(&encode_png(96, 96));
        let header_only = data_uri(&encode_png(96, 96)[..33]);
        let library = serde_json::json!([
            { "id": "a", "name": "Good", "data": good, "futureField": 1 },
            { "id": "b", "name": "Bad", "data": header_only },
            { "id": "c", "name": "Builtin", "data": "claude" },
            { "id": "d", "name": "No data" },
        ]);
        conn.execute(
            "INSERT INTO settings VALUES ('custom_icons', ?1)",
            params![library.to_string()],
        )
        .unwrap();

        assert_eq!(strip_invalid_custom_icons(&conn).unwrap(), 3);

        let raw: String = conn
            .query_row(
                "SELECT value FROM settings WHERE key = 'custom_icons'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            value,
            serde_json::json!([{ "id": "a", "name": "Good", "data": good, "futureField": 1 }])
        );
    }

    #[test]
    fn library_add_dedupes_validates_and_puts_newest_first() {
        let db = Database::memory().unwrap();
        let first = data_uri(&encode_png(96, 96));
        let second = data_uri(&encode_png(64, 64));

        let added = add_library_icon(&db, "  Relay  ", &first).unwrap();
        assert!(!added.existed);
        assert_eq!(added.icon.name, "Relay");

        // 同一张图不重复存，名称也不改
        let again = add_library_icon(&db, "Other", &first).unwrap();
        assert!(again.existed);
        assert_eq!(again.icon.id, added.icon.id);
        assert_eq!(again.icon.name, "Relay");

        add_library_icon(&db, "Second", &second).unwrap();
        let names: Vec<_> = list_library_icons(&db)
            .unwrap()
            .into_iter()
            .map(|icon| icon.name)
            .collect();
        assert_eq!(names, ["Second", "Relay"]);

        assert!(add_library_icon(&db, " ", &second).is_err());
        assert!(add_library_icon(&db, &"x".repeat(MAX_LIBRARY_NAME_CHARS + 1), &second).is_err());
        assert!(add_library_icon(&db, "Name", "claude").is_err());
        let header_only = data_uri(&encode_png(96, 96)[..33]);
        assert!(add_library_icon(&db, "Name", &header_only).is_err());
        assert_eq!(list_library_icons(&db).unwrap().len(), 2);
    }

    #[test]
    fn library_rename_and_delete_keep_unknown_fields() {
        let db = Database::memory().unwrap();
        let good = data_uri(&encode_png(96, 96));
        db.set_setting(
            CUSTOM_ICONS_KEY,
            &serde_json::json!([
                { "id": "a", "name": "Old", "data": good, "createdAt": 1, "futureField": true },
                { "id": "b", "name": "Gone", "data": good },
            ])
            .to_string(),
        )
        .unwrap();

        let renamed = rename_library_icon(&db, "a", "New").unwrap();
        assert_eq!(renamed.name, "New");
        assert!(rename_library_icon(&db, "missing", "New").is_err());
        assert!(rename_library_icon(&db, "a", "").is_err());

        delete_library_icon(&db, "b").unwrap();
        delete_library_icon(&db, "missing").unwrap();

        let raw = db.get_setting(CUSTOM_ICONS_KEY).unwrap().unwrap();
        let value: serde_json::Value = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            value,
            serde_json::json!([
                { "id": "a", "name": "New", "data": good, "createdAt": 1, "futureField": true }
            ])
        );
    }

    #[test]
    fn library_is_capped() {
        let db = Database::memory().unwrap();
        let entries: Vec<_> = (0..MAX_LIBRARY_ICONS)
            .map(|i| CustomIconEntry {
                id: i.to_string(),
                name: i.to_string(),
                data: format!("{CUSTOM_ICON_PREFIX}{i}"),
                created_at: 0,
                extra: serde_json::Map::new(),
            })
            .collect();
        db.save_custom_icons(&entries).unwrap();
        assert!(add_library_icon(&db, "One more", &data_uri(&encode_png(96, 96))).is_err());
    }
}
