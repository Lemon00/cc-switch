use serde_json::{json, Value};
use std::path::Path;

pub(super) const KEYS: &[&str] = &["usage_recording", "proxy_log"];
pub(super) const LABELS: &[&str] = &["代理用量记录", "后台代理日志"];

pub(super) struct SettingsDraft {
    pub base: Value,
    pub values: [bool; 2],
    pub selected: usize,
}

impl SettingsDraft {
    pub fn load(path: &Path) -> Result<Self, String> {
        let base = super::super::read_json_object_or_default(path)?;
        for key in KEYS {
            if base.get(key).is_some_and(|v| !v.is_boolean()) {
                return Err(format!(
                    "cli.json 中 {key} 必须为布尔值；请修正原文件后按 r 刷新"
                ));
            }
        }
        let values = std::array::from_fn(|i| base[KEYS[i]].as_bool().unwrap_or(false));
        Ok(Self {
            base,
            values,
            selected: 0,
        })
    }

    pub fn dirty(&self) -> bool {
        KEYS.iter()
            .enumerate()
            .any(|(i, key)| self.values[i] != self.base[key].as_bool().unwrap_or(false))
    }

    pub fn save(&self, path: &Path) -> Result<(), String> {
        if !self.dirty() {
            return Ok(());
        }
        let _guard = super::super::lock_proxy_control(&path.with_file_name("cli-control.lock"))?;
        let mut current = super::super::read_json_object_or_default(path)?;
        for (i, key) in KEYS.iter().enumerate() {
            if self.values[i] == self.base[key].as_bool().unwrap_or(false) {
                continue;
            }
            if current.get(key) != self.base.get(key)
                && current.get(key) != Some(&json!(self.values[i]))
            {
                return Err(format!("{key} 已被其他进程修改；请取消草稿并刷新"));
            }
            current[*key] = json!(self.values[i]);
        }
        cc_switch_core::config::write_json_file_private(path, &current).map_err(|e| e.to_string())
    }
}
