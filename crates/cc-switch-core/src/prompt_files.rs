use std::path::{Path, PathBuf};

use crate::app_config::AppType;
use crate::codex_config::get_codex_auth_path;
use crate::config::get_claude_settings_path;
use crate::error::AppError;

use crate::opencode_config::get_opencode_dir;

pub const MAX_TEXT_FILE_BYTES: u64 = 8 * 1024 * 1024;
/// Serializes prompt and workspace writes, including their read/compare step.
pub static CONTENT_WRITE_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

pub fn read_text_file(path: &Path) -> Result<Option<String>, AppError> {
    use std::io::Read;
    let metadata = match std::fs::metadata(path) {
        Ok(metadata) => metadata,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(AppError::io(path, error)),
    };
    if !metadata.is_file() || metadata.len() > MAX_TEXT_FILE_BYTES {
        return Err(AppError::InvalidInput(format!(
            "Expected a regular text file of at most 8 MiB: {}",
            path.display()
        )));
    }
    let file = std::fs::File::open(path).map_err(|error| AppError::io(path, error))?;
    let mut content = String::new();
    file.take(MAX_TEXT_FILE_BYTES + 1)
        .read_to_string(&mut content)
        .map_err(|error| AppError::io(path, error))?;
    validate_text_size(&content)?;
    Ok(Some(content))
}

pub fn validate_text_size(content: &str) -> Result<(), AppError> {
    if content.len() as u64 > MAX_TEXT_FILE_BYTES {
        return Err(AppError::InvalidInput(
            "Text file exceeds the 8 MiB limit".into(),
        ));
    }
    Ok(())
}

/// The configured root may itself be a linked user directory. Descendants must
/// not redirect a whitelisted filename through a symlink or Windows junction.
pub fn checked_child_path(root: &Path, relative: &Path) -> Result<PathBuf, AppError> {
    let mut path = root.to_path_buf();
    for component in relative.components() {
        let std::path::Component::Normal(name) = component else {
            return Err(AppError::InvalidInput(
                "Invalid relative content path".into(),
            ));
        };
        path.push(name);
        match std::fs::symlink_metadata(&path) {
            Ok(metadata) => {
                let mut linked = metadata.file_type().is_symlink();
                #[cfg(windows)]
                {
                    use std::os::windows::fs::MetadataExt;
                    linked |= metadata.file_attributes() & 0x400 != 0;
                }
                if linked || (!metadata.is_file() && !metadata.is_dir()) {
                    return Err(AppError::InvalidInput(format!(
                        "Linked or special content paths are not supported: {}",
                        path.display()
                    )));
                }
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(AppError::io(&path, error)),
        }
    }
    Ok(path)
}

pub fn validate_prompt_content(_app: &AppType, content: &str) -> Result<(), AppError> {
    validate_text_size(content)?;

    Ok(())
}

/// 返回指定应用所使用的提示词文件路径。
pub fn prompt_file_path(app: &AppType) -> Result<PathBuf, AppError> {
    let base_dir: PathBuf = match app {
        AppType::Claude => get_base_dir_with_fallback(get_claude_settings_path(), ".claude")?,
        AppType::Codex => get_base_dir_with_fallback(get_codex_auth_path(), ".codex")?,

        AppType::OpenCode => get_opencode_dir(),

        AppType::Dsh => crate::dsh_config::data_dir(),

        // agy 只从各「定制根目录」读独立的 AGENTS.md；全局那一个是 ~/.gemini/config，Key 模式的
        // 私有 --gemini_dir 也经软链共享它。别用 ~/.gemini/GEMINI.md：那是 Gemini CLI 的。
        AppType::Agy => crate::agy_config::customizations_dir(),
        // omp 只读按 profile 切换的 agent 目录里的 AGENTS.md（上游 discovery/builtin.ts）
    };

    let filename = match app {
        AppType::Claude => "CLAUDE.md",
        AppType::Codex => "AGENTS.md",

        AppType::OpenCode => "AGENTS.md",

        AppType::Agy | AppType::Dsh => "AGENTS.md",
    };

    {
        Ok(base_dir.join(filename))
    }
}

/// 把用户主目录前缀换成 `~`，给界面显示用（复制路径仍用完整路径）。
pub fn display_path(path: &Path) -> String {
    display_path_with_home(path, &crate::config::get_home_dir())
}

fn display_path_with_home(path: &Path, home: &Path) -> String {
    match path.strip_prefix(home) {
        Ok(rest) if rest.as_os_str().is_empty() => "~".to_string(),
        Ok(rest) => format!("~{}{}", std::path::MAIN_SEPARATOR, rest.display()),
        Err(_) => path.display().to_string(),
    }
}

fn get_base_dir_with_fallback(
    primary_path: PathBuf,
    fallback_dir: &str,
) -> Result<PathBuf, AppError> {
    primary_path
        .parent()
        .map(|p| p.to_path_buf())
        .or_else(|| dirs::home_dir().map(|h| h.join(fallback_dir)))
        .ok_or_else(|| {
            AppError::localized(
                "home_dir_not_found",
                format!("无法确定 {fallback_dir} 配置目录：用户主目录不存在"),
                format!("Cannot determine {fallback_dir} config directory: user home not found"),
            )
        })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn text_reads_distinguish_missing_empty_invalid_and_oversized_files() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("MEMORY.md");
        assert_eq!(read_text_file(&path).unwrap(), None);
        std::fs::write(&path, "").unwrap();
        assert_eq!(read_text_file(&path).unwrap(), Some(String::new()));
        std::fs::write(&path, "  中文\r\n").unwrap();
        assert_eq!(
            read_text_file(&path).unwrap().as_deref(),
            Some("  中文\r\n")
        );
        std::fs::write(&path, [0xff]).unwrap();
        assert!(read_text_file(&path).is_err());
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(MAX_TEXT_FILE_BYTES + 1).unwrap();
        assert!(read_text_file(&path)
            .unwrap_err()
            .to_string()
            .contains("8 MiB"));
        assert!(read_text_file(root.path()).is_err());
    }

    #[test]
    fn content_paths_reject_traversal() {
        let root = tempfile::tempdir().unwrap();
        for name in ["../MEMORY.md", "/MEMORY.md", "memory/../../outside.md"] {
            assert!(checked_child_path(root.path(), Path::new(name)).is_err());
        }
        assert_eq!(
            checked_child_path(root.path(), Path::new("memory/2026-11-25.md")).unwrap(),
            root.path().join("memory/2026-11-25.md")
        );
    }

    #[cfg(unix)]
    #[test]
    fn content_paths_reject_linked_files_and_directories() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        std::os::unix::fs::symlink(outside.path(), root.path().join("memory")).unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("missing"),
            root.path().join("MEMORY.md"),
        )
        .unwrap();
        assert!(checked_child_path(root.path(), Path::new("memory/new.md")).is_err());
        assert!(checked_child_path(root.path(), Path::new("MEMORY.md")).is_err());
    }

    #[cfg(windows)]
    #[test]
    fn content_paths_reject_windows_directory_junctions() {
        let root = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        let junction = root.path().join("memory");
        let result = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&junction)
            .arg(outside.path())
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        assert!(checked_child_path(root.path(), Path::new("memory/new.md")).is_err());
        std::fs::remove_dir(junction).unwrap();
        assert!(outside.path().is_dir());
    }

    #[test]
    fn display_path_shortens_the_home_prefix_only() {
        let home = Path::new("/Users/me");
        let sep = std::path::MAIN_SEPARATOR;
        assert_eq!(
            display_path_with_home(&home.join(".hermes").join("SOUL.md"), home),
            format!("~{sep}.hermes{sep}SOUL.md")
        );
        assert_eq!(display_path_with_home(home, home), "~");
        assert_eq!(
            display_path_with_home(Path::new("/opt/hermes/SOUL.md"), home),
            "/opt/hermes/SOUL.md"
        );
        // 只认整段目录前缀，不把 /Users/meow 当成 /Users/me 下面
        assert_eq!(
            display_path_with_home(Path::new("/Users/meow/SOUL.md"), home),
            "/Users/meow/SOUL.md"
        );
    }
}
