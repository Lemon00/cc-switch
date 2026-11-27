use super::{snapshot::EnvSnapshot, EnvConflict};

#[cfg(not(target_os = "windows"))]
pub(super) fn prepare_restoration(conflicts: &[EnvConflict]) -> Result<Vec<EnvSnapshot>, String> {
    prepare_file_restoration(conflicts)
}

#[cfg(any(test, not(target_os = "windows")))]
fn prepare_file_restoration(conflicts: &[EnvConflict]) -> Result<Vec<EnvSnapshot>, String> {
    use std::collections::{BTreeMap, BTreeSet};

    let mut groups: BTreeMap<std::path::PathBuf, BTreeMap<&str, &str>> = BTreeMap::new();
    for conflict in conflicts {
        if conflict.source_type != "file" {
            return Err("只能在此系统恢复 shell 配置备份".into());
        }
        super::shell_export_line(&conflict.var_name, &conflict.var_value)?;
        if conflict.var_value.contains(['\r', '\n']) {
            return Err("旧备份包含多行值，无法安全自动恢复，请手动处理".into());
        }
        let (path, _) = super::parse_file_source_path(&conflict.source_path)?;
        let path = std::fs::canonicalize(path)
            .map_err(|error| format!("读取配置文件路径失败: {error}"))?;
        let selections = groups.entry(path).or_default();
        if selections
            .insert(&conflict.var_name, &conflict.var_value)
            .is_some_and(|previous| previous != conflict.var_value)
        {
            return Err(format!(
                "旧备份包含同名变量的不同值，拒绝自动恢复: {}",
                conflict.var_name
            ));
        }
    }
    groups
        .into_iter()
        .map(|(path, selections)| {
            let after = std::fs::read_to_string(&path)
                .map_err(|error| format!("读取配置文件失败 {}: {error}", path.display()))?;
            let exports: BTreeMap<&str, String> = selections
                .iter()
                .map(|(name, value)| Ok((*name, super::shell_export_line(name, value)?)))
                .collect::<Result<_, String>>()?;
            let mut present = BTreeSet::new();
            for line in after.lines() {
                let trimmed = line.trim();
                if trimmed.is_empty() || trimmed.starts_with('#') {
                    continue;
                }
                if let Some((name, value)) = super::snapshot::literal_assignment(line) {
                    if let Some(expected) = selections.get(name) {
                        if value != *expected {
                            return Err(format!("环境变量已存在且值不同，拒绝覆盖: {name}"));
                        }
                        present.insert(name);
                    }
                } else if let Some((name, _)) =
                    exports.iter().find(|(_, export)| export.trim() == trimmed)
                {
                    present.insert(*name);
                } else {
                    return Err(format!(
                        "配置文件包含复杂 shell 语法，旧备份无法安全自动恢复，请手动处理: {}",
                        path.display()
                    ));
                }
            }
            let mut before = after.clone();
            for (name, export) in &exports {
                if !present.contains(name) {
                    before.push_str(export);
                }
            }
            Ok(EnvSnapshot::File {
                path,
                before,
                after,
            })
        })
        .collect()
}

#[cfg(target_os = "windows")]
pub(super) fn prepare_restoration(conflicts: &[EnvConflict]) -> Result<Vec<EnvSnapshot>, String> {
    let mut snapshots = Vec::new();
    let mut seen = std::collections::BTreeMap::new();
    for conflict in conflicts {
        if conflict.source_type != "system" {
            return Err("不能在 Windows 恢复 shell 配置备份".into());
        }
        if conflict.var_name.is_empty()
            || conflict.var_name.contains(['\0', '='])
            || conflict.var_value.contains('\0')
        {
            return Err("旧备份包含无效的环境变量名或值".into());
        }
        let identity = (
            super::registry_is_current_user(&conflict.source_path)?,
            conflict.var_name.to_ascii_lowercase(),
        );
        if let Some(previous) = seen.insert(identity, &conflict.var_value) {
            if previous != &conflict.var_value {
                return Err(format!(
                    "旧备份包含同名变量的不同值，拒绝自动恢复: {}",
                    conflict.var_name
                ));
            }
            continue;
        }
        let key = super::snapshot::registry_key(&conflict.source_path)?;
        let current = match key.get_raw_value(&conflict.var_name) {
            Ok(value) => Some(value),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(format!("读取环境变量失败: {error}")),
        };
        snapshots.push(registry_restoration(conflict, current)?);
    }
    Ok(snapshots)
}

#[cfg(target_os = "windows")]
fn registry_restoration(
    conflict: &EnvConflict,
    current: Option<winreg::RegValue>,
) -> Result<EnvSnapshot, String> {
    use winreg::{enums::REG_EXPAND_SZ, types::ToRegValue};

    let value = if let Some(current) = current {
        if super::super::env_checker::registry_env_value(&current)? != conflict.var_value {
            return Err(format!(
                "环境变量已存在且值不同，拒绝覆盖: {}",
                conflict.var_name
            ));
        }
        current
    } else {
        conflict.var_value.to_reg_value()
    };
    Ok(EnvSnapshot::Registry {
        source_path: conflict.source_path.clone(),
        var_name: conflict.var_name.clone(),
        value_bytes: value.bytes,
        expandable: value.vtype == REG_EXPAND_SZ,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn conflict(path: &std::path::Path, value: &str) -> EnvConflict {
        EnvConflict {
            var_name: "OPENAI_API_KEY".into(),
            var_value: value.into(),
            source_type: "file".into(),
            source_path: format!("{}:1", path.display()),
        }
    }

    #[test]
    fn legacy_file_restore_is_idempotent_and_deduplicates() {
        for value in ["simple", "a'b $HOME; $(id)", ""] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("shellrc");
            std::fs::write(&path, "# keep\r\nOTHER=value\r\n").unwrap();
            let selected = conflict(&path, value);
            let selections = [selected.clone(), selected];
            let snapshots = prepare_file_restoration(&selections).unwrap();
            assert_eq!(snapshots.len(), 1);
            snapshots[0].apply(true).unwrap();
            let restored = std::fs::read_to_string(&path).unwrap();
            assert_eq!(restored.matches("export OPENAI_API_KEY=").count(), 1);
            prepare_file_restoration(&selections).unwrap()[0]
                .apply(true)
                .unwrap();
            assert_eq!(std::fs::read_to_string(path).unwrap(), restored);
        }
    }

    #[test]
    fn legacy_restore_does_not_overwrite_existing_values_or_complex_shell() {
        for content in [
            "OPENAI_API_KEY=new\n",
            "cat <<'EOF'\nOPENAI_API_KEY=old\nEOF\n",
            "printf '%s' '\n",
            "echo continuation \\\n",
        ] {
            let temp = tempfile::tempdir().unwrap();
            let path = temp.path().join("shellrc");
            std::fs::write(&path, content).unwrap();
            assert!(prepare_file_restoration(&[conflict(&path, "old")]).is_err());
            assert_eq!(std::fs::read_to_string(path).unwrap(), content);
        }
    }

    #[test]
    fn legacy_restore_rejects_ambiguous_duplicates_and_multiline_values() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("shellrc");
        std::fs::write(&path, "# keep\n").unwrap();
        assert!(
            prepare_file_restoration(&[conflict(&path, "first"), conflict(&path, "second")])
                .is_err()
        );
        assert!(prepare_file_restoration(&[conflict(&path, "first\nsecond")]).is_err());
        assert_eq!(std::fs::read_to_string(path).unwrap(), "# keep\n");
    }

    #[test]
    fn legacy_restore_prepares_all_files_before_writing() {
        let temp = tempfile::tempdir().unwrap();
        let first = temp.path().join("first");
        let second = temp.path().join("second");
        std::fs::write(&first, "# keep\n").unwrap();
        std::fs::write(&second, "OPENAI_API_KEY=new\n").unwrap();
        assert!(
            prepare_file_restoration(&[conflict(&first, "old"), conflict(&second, "old")]).is_err()
        );
        assert_eq!(std::fs::read_to_string(first).unwrap(), "# keep\n");
    }

    #[cfg(target_os = "windows")]
    #[test]
    fn legacy_registry_restore_preserves_existing_type_and_refuses_conflicts() {
        use winreg::{enums::*, types::ToRegValue, RegValue};
        let selected = EnvConflict {
            var_name: "OPENAI_API_KEY".into(),
            var_value: "%USERPROFILE%\\key".into(),
            source_type: "system".into(),
            source_path: "HKEY_CURRENT_USER\\Environment".into(),
        };
        let mut original = selected.var_value.to_reg_value();
        original.vtype = REG_EXPAND_SZ;
        let bytes = original.bytes.clone();
        let EnvSnapshot::Registry {
            value_bytes,
            expandable,
            ..
        } = registry_restoration(&selected, Some(original)).unwrap()
        else {
            panic!("expected registry snapshot")
        };
        assert_eq!(value_bytes, bytes);
        assert!(expandable);
        assert!(registry_restoration(&selected, Some("new".to_reg_value())).is_err());
        assert!(registry_restoration(
            &selected,
            Some(RegValue {
                bytes: vec![],
                vtype: REG_DWORD
            })
        )
        .is_err());
        let EnvSnapshot::Registry {
            value_bytes,
            expandable,
            ..
        } = registry_restoration(&selected, None).unwrap()
        else {
            panic!("expected registry snapshot")
        };
        assert_eq!(value_bytes, bytes);
        assert!(!expandable);
    }
}
