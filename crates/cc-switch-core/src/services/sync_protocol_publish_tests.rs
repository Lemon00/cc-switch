use super::*;
use reqwest::header::{HeaderMap, IF_MATCH, IF_NONE_MATCH};

#[test]
fn publication_conditions_preserve_the_observed_remote_revision() {
    for (condition, header, expected) in [
        (
            PublishCondition::from_remote(None, false).unwrap(),
            IF_NONE_MATCH,
            "*",
        ),
        (
            PublishCondition::from_remote(Some("\"revision-1\""), true).unwrap(),
            IF_MATCH,
            "\"revision-1\"",
        ),
    ] {
        let mut headers = HeaderMap::new();
        condition.apply(&mut headers).unwrap();
        assert_eq!(headers.get(header).unwrap(), expected);
        assert_eq!(headers.len(), 1);
    }
    let mut headers = HeaderMap::new();
    PublishCondition::Overwrite.apply(&mut headers).unwrap();
    assert!(headers.is_empty());
}

#[test]
fn automatic_publication_rejects_missing_weak_and_multiple_etags() {
    for etag in [
        None,
        Some(""),
        Some("revision"),
        Some("W/\"revision\""),
        Some("\"a\", \"b\""),
        Some("\"a\r\nb\""),
    ] {
        assert!(
            matches!(
                PublishCondition::from_remote(etag, true),
                Err(AppError::Conflict(_))
            ),
            "{etag:?}"
        );
    }
}

fn snapshot_manifest(version: u32, data: &[u8]) -> SyncManifest {
    let artifacts = BTreeMap::from([
        (
            REMOTE_DB_SQL.into(),
            ArtifactMeta {
                sha256: sha256_hex(data),
                size: data.len() as u64,
            },
        ),
        (
            REMOTE_SKILLS_ZIP.into(),
            ArtifactMeta {
                sha256: sha256_hex(b"skills"),
                size: 6,
            },
        ),
    ]);
    SyncManifest {
        format: PROTOCOL_FORMAT.into(),
        version,
        db_compat_version: Some(DB_COMPAT_VERSION),
        device_name: "test".into(),
        created_at: "2026-11-26T00:00:00Z".into(),
        snapshot_id: compute_snapshot_id(&artifacts),
        artifacts,
    }
}

#[test]
fn new_snapshots_do_not_overwrite_artifacts_referenced_by_an_older_manifest() {
    let old = snapshot_manifest(PROTOCOL_VERSION, b"old database");
    let new = snapshot_manifest(PROTOCOL_VERSION, b"new database");
    for manifest in [&old, &new] {
        validate_manifest_compat(manifest, RemoteLayout::Current).unwrap();
    }
    for name in [REMOTE_DB_SQL, REMOTE_SKILLS_ZIP] {
        assert_ne!(
            artifact_path(&old, name).unwrap(),
            artifact_path(&new, name).unwrap()
        );
    }
}

#[test]
fn legacy_snapshots_still_use_the_original_artifact_locations() {
    let manifest = snapshot_manifest(2, b"legacy database");
    validate_manifest_compat(&manifest, RemoteLayout::Current).unwrap();
    for name in [REMOTE_DB_SQL, REMOTE_SKILLS_ZIP] {
        assert_eq!(artifact_path(&manifest, name).unwrap(), name);
    }
}

#[test]
fn artifact_paths_reject_traversal_and_unsupported_versions() {
    let mut manifest = snapshot_manifest(PROTOCOL_VERSION, b"database");
    assert!(artifact_path(&manifest, "../db.sql").is_err());
    manifest.snapshot_id = "../".repeat(22);
    assert!(artifact_path(&manifest, REMOTE_DB_SQL).is_err());
    manifest = snapshot_manifest(PROTOCOL_VERSION + 1, b"database");
    assert!(artifact_path(&manifest, REMOTE_DB_SQL).is_err());
    assert!(validate_manifest_compat(&manifest, RemoteLayout::Current).is_err());
}
