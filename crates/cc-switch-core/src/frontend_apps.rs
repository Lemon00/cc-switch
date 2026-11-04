//! Frontend app whitelist configuration (P4, decouple-core).
//!
//! Which apps a frontend exposes is configuration, not code. Each frontend
//! reads its list from `<app-config-dir>/frontend-apps.json`:
//!
//! ```json
//! { "cli": ["codex", "agy"], "tui": ["codex"], "gui": ["*"] }
//! ```
//!
//! Rules (a bad file must never brick startup, but it must not be silent either):
//! - Missing file or missing frontend key -> built-in default for that frontend.
//! - Unreadable file or invalid JSON -> built-in default, with a warning.
//! - `"*"` -> every app the frontend can actually run (`default_apps`), so a CLI
//!   that only implements codex/agy does not silently accept `--app claude`.
//! - Unknown names are dropped with a warning; an explicitly empty list (or one
//!   where nothing parses) stays empty — the caller reports "no app allowed"
//!   instead of silently re-opening the default set.
//! - Built-in defaults: `cli` -> [codex, agy] (the A-scheme set); any other
//!   frontend -> all apps.
//!
//! A future TUI gets its list from this same file; it does not need a code
//! change to pick its apps. The GUI does not read this file today (it shows
//! all apps by design); if it ever needs a whitelist, it reads the `gui` key.

use std::collections::HashMap;
use std::fs;
use std::path::PathBuf;

use crate::app_config::AppType;
use crate::config::get_app_config_dir;

/// Whitelist file name inside the app config dir (same dir as cc-switch.db).
pub const WHITELIST_FILE: &str = "frontend-apps.json";

/// Built-in default app list for a frontend.
pub fn default_apps(frontend: &str) -> Vec<AppType> {
    match frontend {
        "cli" => vec![AppType::Codex, AppType::Agy],
        _ => AppType::all().collect(),
    }
}

/// Parse one app name as used in the whitelist file. Accepts the serde names
/// (`claude-desktop`, `grokbuild`, `opencode`, …) plus the CLI's agy aliases.
pub fn parse_app_name(name: &str) -> Option<AppType> {
    match name {
        "agy" | "antigravity" | "antigravity-cli" => Some(AppType::Agy),
        other => {
            serde_json::from_value::<AppType>(serde_json::Value::String(other.to_string())).ok()
        }
    }
}

/// Full path of the whitelist file.
pub fn whitelist_path() -> PathBuf {
    get_app_config_dir().join(WHITELIST_FILE)
}

/// Load the app whitelist for a frontend, with the fallback rules documented
/// at the top of this module. Never fails: any problem yields the default.
pub fn load_whitelist(frontend: &str) -> Vec<AppType> {
    let path = whitelist_path();
    let Ok(text) = fs::read_to_string(&path) else {
        return default_apps(frontend);
    };
    let map = match serde_json::from_str::<HashMap<String, Vec<String>>>(&text) {
        Ok(map) => map,
        Err(e) => {
            log::warn!("解析 {} 失败，按内置默认应用列表处理: {e}", path.display());
            return default_apps(frontend);
        }
    };
    let Some(names) = map.get(frontend) else {
        return default_apps(frontend);
    };
    if names.iter().any(|n| n == "*") {
        // `*` = 该前端「能跑的全部应用」，而不是全部 14 个：CLI 没有实现的
        // 应用即使写进白名单也只会在命令分支里撞到未实现。
        return default_apps(frontend);
    }
    let mut apps: Vec<AppType> = Vec::new();
    for name in names {
        match parse_app_name(name) {
            Some(app) => {
                if !apps.contains(&app) {
                    apps.push(app);
                }
            }
            None => log::warn!(
                "{} 的 {frontend} 一项里有未知应用名 {name}，已忽略",
                path.display()
            ),
        }
    }
    // 显式空列表 / 名称全部无效时保持为空：调用方据此报错，不再悄悄回退默认。
    apps
}
