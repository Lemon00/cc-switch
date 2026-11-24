use regex::Regex;
use std::io::Read;
use std::sync::LazyLock;
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::config::write_text_file;
use crate::openclaw_config::get_openclaw_dir;

#[derive(serde::Deserialize)]
pub struct WorkspaceExpectedContent {
    content: Option<String>,
}

static WORKSPACE_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

fn write_workspace_checked(
    path: &std::path::Path,
    content: &str,
    expected: &WorkspaceExpectedContent,
) -> Result<(), String> {
    let _guard = WORKSPACE_WRITE_LOCK
        .lock()
        .map_err(|error| error.to_string())?;
    let current = match std::fs::read_to_string(path) {
        Ok(content) => Some(content),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
        Err(error) => return Err(error.to_string()),
    };
    if current != expected.content {
        return Err("文件已被其他操作修改，请保留草稿并重新打开文件后再保存 (File changed; reload before saving)".to_string());
    }
    write_text_file(path, content).map_err(|error| error.to_string())
}

/// Allowed workspace filenames (whitelist for security)
const ALLOWED_FILES: &[&str] = &[
    "AGENTS.md",
    "SOUL.md",
    "USER.md",
    "IDENTITY.md",
    "TOOLS.md",
    "MEMORY.md",
    "HEARTBEAT.md",
    "BOOTSTRAP.md",
    "BOOT.md",
];

fn validate_filename(filename: &str) -> Result<(), String> {
    if !ALLOWED_FILES.contains(&filename) {
        return Err(format!(
            "Invalid workspace filename: {filename}. Allowed: {}",
            ALLOWED_FILES.join(", ")
        ));
    }
    Ok(())
}

// --- Daily memory files (memory/YYYY-MM-DD.md) ---

static DAILY_MEMORY_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^\d{4}-\d{2}-\d{2}\.md$").unwrap());

fn validate_daily_memory_filename(filename: &str) -> Result<(), String> {
    if !DAILY_MEMORY_RE.is_match(filename) {
        return Err(format!(
            "Invalid daily memory filename: {filename}. Expected: YYYY-MM-DD.md"
        ));
    }
    Ok(())
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyMemoryFileInfo {
    pub filename: String,
    pub date: String,
    pub size_bytes: u64,
    pub modified_at: u64,
    pub preview: String,
}

// --- Daily memory commands ---

/// List all daily memory files under `workspace/memory/`.
#[tauri::command]
pub async fn list_daily_memory_files() -> Result<Vec<DailyMemoryFileInfo>, String> {
    tauri::async_runtime::spawn_blocking(|| {
        list_daily_memory_files_in(&get_openclaw_dir().join("workspace").join("memory"))
    })
    .await
    .map_err(|error| format!("Failed to list daily memory files: {error}"))?
}

fn daily_memory_preview(reader: impl Read) -> std::io::Result<String> {
    let mut bytes = Vec::new();
    reader.take(200 * 4).read_to_end(&mut bytes)?;
    Ok(String::from_utf8_lossy(&bytes).chars().take(200).collect())
}

fn list_daily_memory_files_in(
    memory_dir: &std::path::Path,
) -> Result<Vec<DailyMemoryFileInfo>, String> {
    if !memory_dir.exists() {
        return Ok(Vec::new());
    }

    let mut files: Vec<DailyMemoryFileInfo> = Vec::new();

    let entries = std::fs::read_dir(&memory_dir)
        .map_err(|e| format!("Failed to read memory directory: {e}"))?;

    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_string();
        if validate_daily_memory_filename(&name).is_err() {
            continue;
        }

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => continue,
        };
        if !meta.is_file() {
            continue;
        }

        let date = name.trim_end_matches(".md").to_string();

        let size_bytes = meta.len();
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        let preview = std::fs::File::open(entry.path())
            .and_then(daily_memory_preview)
            .unwrap_or_default();

        files.push(DailyMemoryFileInfo {
            filename: name,
            date,
            size_bytes,
            modified_at,
            preview,
        });
    }

    // Sort by filename descending (newest date first, YYYY-MM-DD.md)
    files.sort_by(|a, b| b.filename.cmp(&a.filename));

    Ok(files)
}

/// Read a daily memory file.
#[tauri::command]
pub async fn read_daily_memory_file(filename: String) -> Result<Option<String>, String> {
    validate_daily_memory_filename(&filename)?;

    let path = get_openclaw_dir()
        .join("workspace")
        .join("memory")
        .join(&filename);

    if !path.exists() {
        return Ok(None);
    }

    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|e| format!("Failed to read daily memory file {filename}: {e}"))
}

/// Write a daily memory file (atomic write).
#[tauri::command]
pub async fn write_daily_memory_file(
    filename: String,
    content: String,
    expected: WorkspaceExpectedContent,
) -> Result<(), String> {
    validate_daily_memory_filename(&filename)?;

    let memory_dir = get_openclaw_dir().join("workspace").join("memory");

    std::fs::create_dir_all(&memory_dir)
        .map_err(|e| format!("Failed to create memory directory: {e}"))?;

    let path = memory_dir.join(&filename);

    write_workspace_checked(&path, &content, &expected)
        .map_err(|e| format!("Failed to write daily memory file {filename}: {e}"))
}

fn fold_char(character: char) -> impl Iterator<Item = char> {
    character.to_lowercase().map(|folded| {
        if folded == '\u{03c2}' {
            '\u{03c3}'
        } else {
            folded
        }
    })
}

fn fold_search_text(original: &str) -> String {
    original.chars().flat_map(fold_char).collect()
}

fn original_search_offset(original: &str, folded_offset: usize) -> usize {
    let mut offset = 0;
    for (position, character) in original.char_indices() {
        offset += fold_char(character).map(char::len_utf8).sum::<usize>();
        if offset > folded_offset {
            return position;
        }
    }
    original.len()
}

#[cfg(test)]
mod regression_tests {
    use super::*;

    #[test]
    fn memory_search_rejects_oversized_files_and_superseded_work() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("2026-11-23.md");
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(8 * 1024 * 1024 + 1).unwrap();
        assert!(
            search_daily_memory_files_in(directory.path(), "note", || true)
                .unwrap_err()
                .contains("8 MiB")
        );
        assert!(
            search_daily_memory_files_in(directory.path(), "note", || false)
                .unwrap_err()
                .contains("superseded")
        );
        drop(file);
        std::fs::write(&path, "İneedle 中文 大小写").unwrap();
        let result = search_daily_memory_files_in(directory.path(), "NEEDLE", || true).unwrap();
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].match_count, 1);
        assert!(result[0].snippet.contains("İneedle"));
    }

    #[test]
    fn memory_preview_has_bounded_reads_and_preserves_unicode() {
        let mut reader = std::io::Cursor::new("😀".repeat(1000).into_bytes());
        assert_eq!(daily_memory_preview(&mut reader).unwrap(), "😀".repeat(200));
        assert_eq!(reader.position(), 800);
        let mut reader = std::io::Cursor::new(vec![b'a'; 1_000_000]);
        assert_eq!(daily_memory_preview(&mut reader).unwrap(), "a".repeat(200));
        assert_eq!(reader.position(), 800);
    }

    #[test]
    fn unicode_search_uses_identical_folding_and_original_offsets() {
        for (content, query) in [("ΟΣ", "ΟΣ"), ("ΟΣ", "ος"), ("İneedle", "needle")] {
            let haystack = fold_search_text(content);
            let query = fold_search_text(query);
            let position = haystack.find(&query).unwrap();
            let original = original_search_offset(content, position);
            assert!(content.is_char_boundary(original));
            if content == "İneedle" {
                assert_eq!(&content[original..], "needle");
            }
        }
    }

    #[test]
    fn stale_workspace_writes_preserve_external_edits_and_new_files() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("MEMORY.md");
        std::fs::write(&path, "external").unwrap();
        for content in [None, Some("original".to_string())] {
            assert!(
                write_workspace_checked(&path, "draft", &WorkspaceExpectedContent { content })
                    .is_err()
            );
            assert_eq!(std::fs::read_to_string(&path).unwrap(), "external");
        }
        write_workspace_checked(
            &path,
            "saved",
            &WorkspaceExpectedContent {
                content: Some("external".into()),
            },
        )
        .unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "saved");
    }
}

/// Find the largest index `<= i` that is a valid UTF-8 char boundary.
/// Equivalent to the unstable `str::floor_char_boundary` (stabilized in 1.91).
fn floor_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i -= 1;
    }
    i
}

/// Find the smallest index `>= i` that is a valid UTF-8 char boundary.
/// Equivalent to the unstable `str::ceil_char_boundary` (stabilized in 1.91).
fn ceil_char_boundary(s: &str, mut i: usize) -> usize {
    if i >= s.len() {
        return s.len();
    }
    while !s.is_char_boundary(i) {
        i += 1;
    }
    i
}

/// Search result for daily memory full-text search.
#[derive(Debug, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DailyMemorySearchResult {
    pub filename: String,
    pub date: String,
    pub size_bytes: u64,
    pub modified_at: u64,
    pub snippet: String,
    pub match_count: usize,
}

/// Full-text search across all daily memory files.
///
/// Performs case-insensitive search on both the date field and file content.
/// Returns results sorted by filename descending (newest first), each with a
/// snippet showing ~120 characters of context around the first match.
#[tauri::command]
pub async fn search_daily_memory_files(
    query: String,
) -> Result<Vec<DailyMemorySearchResult>, String> {
    static GENERATION: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    static LIMIT: LazyLock<std::sync::Arc<tokio::sync::Semaphore>> =
        LazyLock::new(|| std::sync::Arc::new(tokio::sync::Semaphore::new(1)));
    let generation = GENERATION.fetch_add(1, std::sync::atomic::Ordering::SeqCst) + 1;
    let permit = LIMIT
        .clone()
        .acquire_owned()
        .await
        .map_err(|error| error.to_string())?;
    let is_current = move || GENERATION.load(std::sync::atomic::Ordering::SeqCst) == generation;
    if !is_current() {
        return Err("Search superseded".to_string());
    }
    tauri::async_runtime::spawn_blocking(move || {
        let _permit = permit;
        search_daily_memory_files_in(
            &get_openclaw_dir().join("workspace").join("memory"),
            &query,
            is_current,
        )
    })
    .await
    .map_err(|error| format!("Failed to search daily memory files: {error}"))?
}

fn search_daily_memory_files_in(
    memory_dir: &std::path::Path,
    query: &str,
    is_current: impl Fn() -> bool,
) -> Result<Vec<DailyMemorySearchResult>, String> {
    const MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;
    const MAX_SCAN_BYTES: u64 = 64 * 1024 * 1024;
    const MAX_SCAN_FILES: usize = 10_000;
    let started = std::time::Instant::now();
    let mut scanned_bytes = 0;
    if !memory_dir.exists() || query.is_empty() {
        return Ok(Vec::new());
    }

    if query.len() > 4096 {
        return Err("Search query is too long".to_string());
    }
    let query_lower = fold_search_text(query);
    let mut results: Vec<DailyMemorySearchResult> = Vec::new();

    let entries = std::fs::read_dir(&memory_dir)
        .map_err(|e| format!("Failed to read memory directory: {e}"))?;

    for (index, entry) in entries.enumerate() {
        if !is_current() {
            return Err("Search superseded".to_string());
        }
        if index >= MAX_SCAN_FILES || started.elapsed() > std::time::Duration::from_secs(10) {
            return Err("Memory search exceeded its scan budget".to_string());
        }
        let entry = entry.map_err(|error| format!("Failed to read memory entry: {error}"))?;
        let name = entry.file_name().to_string_lossy().to_string();
        if validate_daily_memory_filename(&name).is_err() {
            continue;
        }
        if !entry
            .file_type()
            .map_err(|error| error.to_string())?
            .is_file()
        {
            continue;
        }
        let meta = entry.metadata().map_err(|error| error.to_string())?;
        if meta.len() > MAX_FILE_BYTES {
            return Err(format!(
                "Memory file exceeds the 8 MiB search limit: {name}"
            ));
        }

        let date = name.trim_end_matches(".md").to_string();
        let mut content = String::new();
        std::fs::File::open(entry.path())
            .map_err(|error| error.to_string())?
            .take(MAX_FILE_BYTES + 1)
            .read_to_string(&mut content)
            .map_err(|error| error.to_string())?;
        scanned_bytes += content.len() as u64;
        if content.len() as u64 > MAX_FILE_BYTES || scanned_bytes > MAX_SCAN_BYTES {
            return Err("Memory search exceeded its byte budget".to_string());
        }
        if !is_current() {
            return Err("Search superseded".to_string());
        }
        let content_lower = fold_search_text(&content);

        // Count matches in content
        let mut content_matches = content_lower.match_indices(&query_lower);
        let first_match = content_matches.next().map(|(offset, _)| offset);
        let match_count = usize::from(first_match.is_some()) + content_matches.count();

        // Also check date field
        let date_matches = date.to_lowercase().contains(&query_lower);

        if first_match.is_none() && !date_matches {
            continue;
        }

        // Build snippet around first content match (~120 chars of context)
        let snippet = if let Some(first_pos) = first_match {
            let origin_pos = original_search_offset(&content, first_pos);
            let start = if origin_pos > 50 {
                floor_char_boundary(&content, origin_pos - 50)
            } else {
                0
            };
            let end = ceil_char_boundary(&content, (origin_pos + 70).min(content.len()));
            let mut s = String::new();
            if start > 0 {
                s.push_str("...");
            }
            s.push_str(&content[start..end]);
            if end < content.len() {
                s.push_str("...");
            }
            s
        } else {
            // Date-only match — use beginning of file as preview
            let end = ceil_char_boundary(&content, 120.min(content.len()));
            let mut s = content[..end].to_string();
            if end < content.len() {
                s.push_str("...");
            }
            s
        };

        let size_bytes = meta.len();
        let modified_at = meta
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| d.as_secs())
            .unwrap_or(0);

        results.push(DailyMemorySearchResult {
            filename: name,
            date,
            size_bytes,
            modified_at,
            snippet,
            match_count,
        });
    }

    // Sort by filename descending (newest date first)
    results.sort_by(|a, b| b.filename.cmp(&a.filename));

    if !is_current() {
        return Err("Search superseded".to_string());
    }
    Ok(results)
}

/// Delete a daily memory file (idempotent).
#[tauri::command]
pub async fn delete_daily_memory_file(filename: String) -> Result<(), String> {
    validate_daily_memory_filename(&filename)?;

    let path = get_openclaw_dir()
        .join("workspace")
        .join("memory")
        .join(&filename);

    if path.exists() {
        std::fs::remove_file(&path)
            .map_err(|e| format!("Failed to delete daily memory file {filename}: {e}"))?;
    }

    Ok(())
}

// --- Workspace file commands ---

/// Read an OpenClaw workspace file content.
/// Returns None if the file does not exist.
#[tauri::command]
pub async fn read_workspace_file(filename: String) -> Result<Option<String>, String> {
    validate_filename(&filename)?;

    let path = get_openclaw_dir().join("workspace").join(&filename);

    if !path.exists() {
        return Ok(None);
    }

    std::fs::read_to_string(&path)
        .map(Some)
        .map_err(|e| format!("Failed to read workspace file {filename}: {e}"))
}

/// Write content to an OpenClaw workspace file (atomic write).
/// Creates the workspace directory if it does not exist.
#[tauri::command]
pub async fn write_workspace_file(
    filename: String,
    content: String,
    expected: WorkspaceExpectedContent,
) -> Result<(), String> {
    validate_filename(&filename)?;

    let workspace_dir = get_openclaw_dir().join("workspace");

    // Ensure workspace directory exists
    std::fs::create_dir_all(&workspace_dir)
        .map_err(|e| format!("Failed to create workspace directory: {e}"))?;

    let path = workspace_dir.join(&filename);

    write_workspace_checked(&path, &content, &expected)
        .map_err(|e| format!("Failed to write workspace file {filename}: {e}"))
}

/// Open the workspace or memory directory in the system file manager.
/// `subdir`: "workspace" opens `~/.openclaw/workspace/`,
///           "memory" opens `~/.openclaw/workspace/memory/`.
#[tauri::command]
pub async fn open_workspace_directory(handle: AppHandle, subdir: String) -> Result<bool, String> {
    let dir = match subdir.as_str() {
        "memory" => get_openclaw_dir().join("workspace").join("memory"),
        _ => get_openclaw_dir().join("workspace"),
    };

    if !dir.exists() {
        std::fs::create_dir_all(&dir).map_err(|e| format!("Failed to create directory: {e}"))?;
    }

    handle
        .opener()
        .open_path(dir.to_string_lossy().to_string(), None::<String>)
        .map_err(|e| format!("Failed to open directory: {e}"))?;

    Ok(true)
}
