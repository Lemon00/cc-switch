use cc_switch_lib::AppType;
use std::str::FromStr;

#[test]
fn parses_only_supported_apps_and_aliases() {
    for (value, expected) in [
        (" ClAuDe ", AppType::Claude),
        ("codex", AppType::Codex),
        ("opencode", AppType::OpenCode),
        ("deepseek-harness", AppType::Dsh),
        ("antigravity", AppType::Agy),
    ] {
        assert_eq!(AppType::from_str(value).unwrap(), expected);
    }
    for retired in [
        "gemini",
        "grokbuild",
        "claude-desktop",
        "hermes",
        "openclaw",
        "kimi",
        "mcode",
        "pi",
        "omp",
    ] {
        assert!(AppType::from_str(retired).is_err());
    }
}

#[test]
fn unknown_apps_have_an_actionable_error() {
    let message = AppType::from_str("unknown").unwrap_err().to_string();
    assert!(message.contains("可选值") && message.contains("unknown"));
}
