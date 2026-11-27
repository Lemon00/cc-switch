use cc_switch_core::settings::AppSettings;

#[test]
fn independent_release_notice_preserves_upstream_seen_version() {
    let mut settings: AppSettings = serde_json::from_value(serde_json::json!({
        "whatsNewSeenVersion": "4.0.28"
    }))
    .unwrap();
    assert!(settings.independent_whats_new_seen_version.is_none());
    settings.independent_whats_new_seen_version = Some("1.0.0".into());
    let restored: AppSettings =
        serde_json::from_str(&serde_json::to_string(&settings).unwrap()).unwrap();
    assert_eq!(restored.whats_new_seen_version.as_deref(), Some("4.0.28"));
    assert_eq!(
        restored.independent_whats_new_seen_version.as_deref(),
        Some("1.0.0")
    );
}
