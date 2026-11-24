use super::EnvConflict;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Debug, Serialize, Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase"
)]
pub(super) enum EnvSnapshot {
    File {
        path: PathBuf,
        before: String,
        after: String,
    },
    Registry {
        source_path: String,
        var_name: String,
        value_bytes: Vec<u8>,
        expandable: bool,
    },
}

#[cfg(not(target_os = "windows"))]
pub(super) fn prepare_deletions(conflicts: &[EnvConflict]) -> Result<Vec<EnvSnapshot>, String> {
    prepare_file_deletions(conflicts)
}

#[cfg(any(test, not(target_os = "windows")))]
fn prepare_file_deletions(conflicts: &[EnvConflict]) -> Result<Vec<EnvSnapshot>, String> {
    use std::collections::{BTreeMap, BTreeSet};

    if conflicts.is_empty() {
        return Err("未选择需要删除的环境变量".into());
    }
    let mut groups: BTreeMap<PathBuf, Vec<(&EnvConflict, usize)>> = BTreeMap::new();
    for conflict in conflicts {
        if conflict.source_type != "file" {
            return Err("只能删除 shell 配置文件中的环境变量".into());
        }
        let (path, line_num) = super::parse_file_source_path(&conflict.source_path)?;
        let path = std::fs::canonicalize(path)
            .map_err(|error| format!("读取配置文件路径失败: {error}"))?;
        groups.entry(path).or_default().push((conflict, line_num));
    }
    groups
        .into_iter()
        .map(|(path, selections)| {
            let before = std::fs::read_to_string(&path)
                .map_err(|error| format!("读取配置文件失败 {}: {error}", path.display()))?;
            let lines: Vec<&str> = before.split_inclusive('\n').collect();
            let last_selected = selections.iter().map(|(_, line)| *line).max().unwrap_or(0);
            if lines.iter().take(last_selected).any(|line| {
                let trimmed = line.trim();
                !trimmed.is_empty()
                    && !trimmed.starts_with('#')
                    && literal_assignment(line).is_none()
            }) {
                return Err(format!(
                    "所选配置行之前包含复杂 shell 语法，无法安全自动删除，请手动处理: {}",
                    path.display()
                ));
            }
            let mut removed = BTreeSet::new();
            for (conflict, line_num) in selections {
                let line = lines
                    .get(line_num - 1)
                    .ok_or_else(|| stale_source(conflict))?;
                let (name, value) = literal_assignment(line).ok_or_else(|| {
                    format!(
                        "配置行已变化或包含复杂 shell 语法，请重新检查或手动处理: {}",
                        conflict.source_path
                    )
                })?;
                if name != conflict.var_name || value != conflict.var_value {
                    return Err(stale_source(conflict));
                }
                removed.insert(line_num - 1);
            }
            let after = lines
                .into_iter()
                .enumerate()
                .filter_map(|(index, line)| (!removed.contains(&index)).then_some(line))
                .collect();
            Ok(EnvSnapshot::File {
                path,
                before,
                after,
            })
        })
        .collect()
}

fn stale_source(conflict: &EnvConflict) -> String {
    format!(
        "环境变量已变化，请重新检查后再操作: {} ({})",
        conflict.var_name, conflict.source_path
    )
}

#[cfg(any(test, not(target_os = "windows")))]
pub(super) fn literal_assignment(line: &str) -> Option<(&str, &str)> {
    let assignment = line.trim().strip_prefix("export ").unwrap_or(line.trim());
    let (name, expression) = assignment.split_once('=')?;
    if name.is_empty()
        || !name.bytes().enumerate().all(|(index, byte)| {
            byte == b'_' || byte.is_ascii_alphabetic() || (index > 0 && byte.is_ascii_digit())
        })
    {
        return None;
    }
    let value = if let Some(quoted) = expression.strip_prefix('\'') {
        let inner = quoted.strip_suffix('\'')?;
        if inner.contains('\'') {
            return None;
        }
        inner
    } else if let Some(quoted) = expression.strip_prefix('"') {
        let inner = quoted.strip_suffix('"')?;
        if inner.contains(['"', '\\', '$', '`']) {
            return None;
        }
        inner
    } else {
        if expression.chars().any(|character| {
            character.is_whitespace()
                || matches!(
                    character,
                    '\'' | '"'
                        | '\\'
                        | '$'
                        | '`'
                        | ';'
                        | '&'
                        | '|'
                        | '<'
                        | '>'
                        | '('
                        | ')'
                        | '{'
                        | '}'
                        | '#'
                        | '~'
                )
        }) {
            return None;
        }
        expression
    };
    (!value.contains(['\0', '\r', '\n'])).then_some((name, value))
}

#[cfg(target_os = "windows")]
pub(super) fn registry_key(source_path: &str) -> Result<winreg::RegKey, String> {
    use winreg::{enums::*, RegKey};

    let (root, path) = if super::registry_is_current_user(source_path)? {
        (HKEY_CURRENT_USER, "Environment")
    } else {
        (
            HKEY_LOCAL_MACHINE,
            "SYSTEM\\CurrentControlSet\\Control\\Session Manager\\Environment",
        )
    };
    RegKey::predef(root)
        .open_subkey_with_flags(path, KEY_QUERY_VALUE | KEY_SET_VALUE)
        .map_err(|error| format!("打开环境变量注册表失败: {error}"))
}

#[cfg(target_os = "windows")]
pub(super) fn prepare_deletions(conflicts: &[EnvConflict]) -> Result<Vec<EnvSnapshot>, String> {
    use std::collections::BTreeSet;
    use winreg::enums::*;

    if conflicts.is_empty() {
        return Err("未选择需要删除的环境变量".into());
    }
    let mut snapshots = Vec::new();
    let mut seen = BTreeSet::new();
    for conflict in conflicts {
        if conflict.source_type != "system" {
            return Err("Windows 只支持清理注册表中的环境变量".into());
        }
        let key = registry_key(&conflict.source_path)?;
        let value = key
            .get_raw_value(&conflict.var_name)
            .map_err(|error| format!("读取环境变量失败 {}: {error}", conflict.var_name))?;
        if !matches!(value.vtype, REG_SZ | REG_EXPAND_SZ) {
            return Err(format!("不支持的环境变量注册表类型: {}", conflict.var_name));
        }
        if super::super::env_checker::registry_env_value(&value)
            .map_err(|error| format!("无法读取环境变量 {}: {error}", conflict.var_name))?
            != conflict.var_value
        {
            return Err(stale_source(conflict));
        }
        if seen.insert((
            conflict.source_path.to_ascii_lowercase(),
            conflict.var_name.to_ascii_lowercase(),
        )) {
            snapshots.push(EnvSnapshot::Registry {
                source_path: conflict.source_path.clone(),
                var_name: conflict.var_name.clone(),
                value_bytes: value.bytes,
                expandable: value.vtype == REG_EXPAND_SZ,
            });
        }
    }
    Ok(snapshots)
}

impl EnvSnapshot {
    pub(super) fn validate(&self, restoring: bool) -> Result<bool, String> {
        match self {
            Self::File {
                path,
                before,
                after,
            } => {
                #[cfg(any(test, not(target_os = "windows")))]
                {
                    let current = std::fs::read_to_string(path)
                        .map_err(|error| format!("读取配置文件失败 {}: {error}", path.display()))?;
                    let (expected, desired) = if restoring {
                        (after, before)
                    } else {
                        (before, after)
                    };
                    if &current == desired {
                        return Ok(false);
                    }
                    if &current != expected {
                        return Err(format!("配置文件已被修改，拒绝覆盖: {}", path.display()));
                    }
                    Ok(true)
                }
                #[cfg(all(not(test), target_os = "windows"))]
                {
                    let _ = (path, before, after, restoring);
                    Err("不能在 Windows 恢复 shell 配置备份".into())
                }
            }
            Self::Registry {
                source_path,
                var_name,
                value_bytes,
                expandable,
            } => {
                #[cfg(target_os = "windows")]
                {
                    let key = registry_key(source_path)?;
                    let current = match key.get_raw_value(var_name) {
                        Ok(value) => Some(value),
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
                        Err(error) => return Err(format!("读取环境变量失败: {error}")),
                    };
                    let original = registry_value(value_bytes, *expandable);
                    let unchanged = current.as_ref().is_some_and(|value| {
                        value.bytes == original.bytes && value.vtype == original.vtype
                    });
                    if (restoring && unchanged) || (!restoring && current.is_none()) {
                        return Ok(false);
                    }
                    if (restoring && current.is_none()) || (!restoring && unchanged) {
                        return Ok(true);
                    }
                    Err(format!("环境变量已被修改，拒绝覆盖: {var_name}"))
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (source_path, var_name, value_bytes, expandable, restoring);
                    Err("不能在此系统恢复 Windows 注册表备份".into())
                }
            }
        }
    }

    pub(super) fn apply(&self, restoring: bool) -> Result<(), String> {
        if !self.validate(restoring)? {
            return Ok(());
        }
        match self {
            Self::File {
                path,
                before,
                after,
            } => {
                let content = if restoring { before } else { after };
                crate::config::atomic_write(path, content.as_bytes())
                    .map_err(|error| format!("写入配置文件失败: {error}"))
            }
            Self::Registry {
                source_path,
                var_name,
                value_bytes,
                expandable,
            } => {
                #[cfg(target_os = "windows")]
                {
                    let key = registry_key(source_path)?;
                    let result = if restoring {
                        key.set_raw_value(var_name, &registry_value(value_bytes, *expandable))
                    } else {
                        key.delete_value(var_name)
                    };
                    result.map_err(|error| format!("写入环境变量注册表失败: {error}"))
                }
                #[cfg(not(target_os = "windows"))]
                {
                    let _ = (source_path, var_name, value_bytes, expandable, restoring);
                    Err("不能在此系统恢复 Windows 注册表备份".into())
                }
            }
        }
    }
}

#[cfg(target_os = "windows")]
fn registry_value(bytes: &[u8], expandable: bool) -> winreg::RegValue {
    use winreg::enums::*;
    winreg::RegValue {
        bytes: bytes.to_vec(),
        vtype: if expandable { REG_EXPAND_SZ } else { REG_SZ },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conflict(path: &std::path::Path, line: usize, value: &str) -> EnvConflict {
        EnvConflict {
            var_name: "OPENAI_API_KEY".into(),
            var_value: value.into(),
            source_type: "file".into(),
            source_path: format!("{}:{line}", path.display()),
        }
    }

    #[test]
    fn deletes_only_selected_occurrence_and_restores_exact_content_once() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shellrc");
        let original = "# keep\r\nexport OPENAI_API_KEY='first'\r\nOPENAI_API_KEY=second\r\nlast";
        std::fs::write(&path, original).unwrap();
        let snapshots = prepare_file_deletions(&[conflict(&path, 2, "first")]).unwrap();
        snapshots[0].apply(false).unwrap();
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "# keep\r\nOPENAI_API_KEY=second\r\nlast"
        );
        snapshots[0].apply(true).unwrap();
        snapshots[0].apply(true).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), original);
    }

    #[test]
    fn groups_deletions_before_line_numbers_shift_and_deduplicates_selections() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shellrc");
        std::fs::write(
            &path,
            "OPENAI_API_KEY=first\n# keep\nOPENAI_API_KEY=second\n",
        )
        .unwrap();
        let snapshots = prepare_file_deletions(&[
            conflict(&path, 1, "first"),
            conflict(&path, 3, "second"),
            conflict(&path, 1, "first"),
        ])
        .unwrap();
        assert_eq!(snapshots.len(), 1);
        snapshots[0].apply(false).unwrap();
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# keep\n");
    }

    #[test]
    fn stale_or_invalid_selection_leaves_all_files_untouched() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        let original = "OPENAI_API_KEY=current\n";
        std::fs::write(&first, original).unwrap();
        std::fs::write(&second, original).unwrap();
        for selection in [
            conflict(&second, 1, "old"),
            conflict(&second, 2, "current"),
            conflict(&second, 0, "current"),
        ] {
            assert!(prepare_file_deletions(&[conflict(&first, 1, "current"), selection]).is_err());
            assert_eq!(std::fs::read_to_string(&first).unwrap(), original);
            assert_eq!(std::fs::read_to_string(&second).unwrap(), original);
        }
    }

    #[test]
    fn refuses_changes_after_snapshot_instead_of_overwriting_them() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shellrc");
        let original = "OPENAI_API_KEY=value\n";
        std::fs::write(&path, original).unwrap();
        let snapshots = prepare_file_deletions(&[conflict(&path, 1, "value")]).unwrap();
        std::fs::write(&path, "changed before deletion\n").unwrap();
        assert!(snapshots[0].apply(false).is_err());
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "changed before deletion\n"
        );
        std::fs::write(&path, original).unwrap();
        snapshots[0].apply(false).unwrap();
        std::fs::write(&path, "changed after deletion\n").unwrap();
        assert!(snapshots[0].apply(true).is_err());
        assert_eq!(
            std::fs::read_to_string(path).unwrap(),
            "changed after deletion\n"
        );
    }

    #[test]
    fn refuses_assignment_like_text_in_complex_shell_contexts() {
        for prefix in [
            "cat <<'EOF'\n",
            "printf '%s' '\n",
            "echo ignored \\\n",
            "configure() {\n",
            "if false; then\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("shellrc");
            let content = format!("{prefix}OPENAI_API_KEY=value\n");
            std::fs::write(&path, &content).unwrap();
            assert!(prepare_file_deletions(&[conflict(&path, 2, "value")]).is_err());
            assert_eq!(std::fs::read_to_string(path).unwrap(), content);
        }
    }

    #[test]
    fn refuses_compound_and_multiline_shell_statements() {
        for line in [
            "OPENAI_API_KEY=value; echo keep",
            "OPENAI_API_KEY=value OTHER=keep",
            "OPENAI_API_KEY='first",
            "OPENAI_API_KEY=\"$HOME\"",
            "OPENAI_API_KEY=$(command)",
            "OPENAI_API_KEY=value\\",
            "OPENAI_API_KEY = value",
        ] {
            assert!(literal_assignment(line).is_none(), "{line}");
        }
        assert_eq!(
            literal_assignment("export OPENAI_API_KEY='a b $HOME'\r\n"),
            Some(("OPENAI_API_KEY", "a b $HOME"))
        );
        assert_eq!(
            literal_assignment("OPENAI_API_KEY=\"value\""),
            Some(("OPENAI_API_KEY", "value"))
        );
        assert_eq!(
            literal_assignment("OPENAI_API_KEY="),
            Some(("OPENAI_API_KEY", ""))
        );
    }

    #[test]
    fn snapshot_roundtrip_keeps_original_shell_syntax() {
        let snapshot = EnvSnapshot::File {
            path: PathBuf::from("shellrc"),
            before: "export OPENAI_API_KEY='a b $HOME'\r\n".into(),
            after: String::new(),
        };
        let encoded = serde_json::to_string(&snapshot).unwrap();
        let decoded: EnvSnapshot = serde_json::from_str(&encoded).unwrap();
        assert_eq!(serde_json::to_string(&decoded).unwrap(), encoded);
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn registry_snapshot_preserves_expandable_type_and_raw_bytes() {
        use winreg::{enums::*, types::ToRegValue};
        let original = "%USERPROFILE%\\secret".to_reg_value();
        for expandable in [true, false] {
            let snapshot = EnvSnapshot::Registry {
                source_path: "HKEY_CURRENT_USER\\Environment".into(),
                var_name: "OPENAI_API_KEY".into(),
                value_bytes: original.bytes.clone(),
                expandable,
            };
            let encoded = serde_json::to_string(&snapshot).unwrap();
            let EnvSnapshot::Registry {
                value_bytes,
                expandable,
                ..
            } = serde_json::from_str(&encoded).unwrap()
            else {
                panic!("registry snapshot expected");
            };
            let restored = registry_value(&value_bytes, expandable);
            assert_eq!(restored.bytes, original.bytes);
            assert_eq!(
                restored.vtype,
                if expandable { REG_EXPAND_SZ } else { REG_SZ }
            );
        }
    }
}
