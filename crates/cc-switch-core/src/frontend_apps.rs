//! Frontend app whitelist configuration (P4, decouple-core).
//!
//! Which apps a frontend exposes is configuration, not code. Each frontend
//! reads its list from `<app-config-dir>/frontend-apps.json`:
//!
//! ```json
//! { "cli": ["codex", "agy"], "tui": ["codex"], "gui": ["*"] }
//! ```
//!
//! Rules (all failures fall back silently, never brick startup):
//! - Missing file, unreadable file, or invalid JSON -> built-in default.
//! - Missing frontend key -> built-in default for that frontend.
//! - `"*"` in a list means all apps.
//! - Unknown names are ignored; if nothing valid remains -> built-in default.
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
    let Ok(text) = fs::read_to_string(whitelist_path()) else {
        return default_apps(frontend);
    };
    let Ok(map) = serde_json::from_str::<HashMap<String, Vec<String>>>(&text) else {
        return default_apps(frontend);
    };
    let Some(names) = map.get(frontend) else {
        return default_apps(frontend);
    };
    if names.iter().any(|n| n == "*") {
        return AppType::all().collect();
    }
    let mut apps: Vec<AppType> = Vec::new();
    for name in names {
        if let Some(app) = parse_app_name(name) {
            if !apps.contains(&app) {
                apps.push(app);
            }
        }
    }
    if apps.is_empty() {
        return default_apps(frontend);
    }
    apps
}
