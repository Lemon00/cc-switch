fn repo_skill(directory: &str) -> InstalledSkill {
    let mut skill = poisoned_skill(&format!("owner/repo:{directory}"), directory);
    skill.name = directory.into();
    skill.repo_owner = Some("owner".into());
    skill.repo_name = Some("repo".into());
    skill.repo_branch = Some("main".into());
    skill.apps.claude = true;
    skill
}

#[test]
#[serial_test::serial]
fn migration_conflict_preserves_every_old_source_and_setting() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let db = Arc::new(Database::memory().unwrap());
    let old = SkillService::get_ssot_dir().unwrap();
    for name in ["a-good", "z-conflict"] {
        let mut skill = poisoned_skill(name, name);
        skill.name = name.into();
        db.save_skill(&skill).unwrap();
        write_skill(&old.join(name), "original");
    }
    let new = home.path().join(".agents/skills");
    write_skill(&new.join("z-conflict"), "external");
    let result = SkillService::migrate_storage(&db, SkillStorageLocation::Unified).unwrap();
    assert!(!result.location_changed);
    assert_eq!(result.migrated_count, 0);
    assert!(!result.errors.is_empty());
    assert_eq!(SkillService::get_ssot_dir().unwrap(), old);
    assert!(old.join("a-good/SKILL.md").is_file());
    assert!(old.join("z-conflict/SKILL.md").is_file());
    assert!(!new.join("a-good").exists());
    assert!(fs::read_to_string(new.join("z-conflict/SKILL.md"))
        .unwrap()
        .contains("external"));
}

#[test]
#[serial_test::serial]
fn migration_setting_failure_rolls_back_files_and_native_links() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let db = Arc::new(Database::memory().unwrap());
    let skill = repo_skill("skill");
    db.save_skill(&skill).unwrap();
    let source = SkillService::get_ssot_dir().unwrap().join("skill");
    write_skill(&source, "original");
    SkillService::sync_to_app_dir("skill", &AppType::Claude).unwrap();
    let native = SkillService::get_app_skills_dir(&AppType::Claude)
        .unwrap()
        .join("skill");
    let settings = home.path().join(".cc-switch/settings.json");
    let saved = fs::read(&settings).unwrap();
    fs::remove_file(&settings).unwrap();
    fs::create_dir(&settings).unwrap();
    let result = SkillService::migrate_storage(&db, SkillStorageLocation::Unified);
    fs::remove_dir(&settings).unwrap();
    fs::write(&settings, saved).unwrap();
    assert!(result.is_err());
    assert!(source.join("SKILL.md").is_file());
    assert!(native.join("SKILL.md").is_file());
    assert!(!home.path().join(".agents/skills/skill").exists());
    assert_eq!(
        crate::settings::get_skill_storage_location(),
        SkillStorageLocation::CcSwitch
    );
}

#[tokio::test]
#[serial_test::serial]
async fn install_after_concurrent_migration_uses_active_storage() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let remote = tempdir().unwrap();
    write_skill(&remote.path().join("new-skill"), "new");
    let service = SkillService {
        repo_fixture: Some(remote.path().into()),
        ..SkillService::new()
    };
    let db = Arc::new(Database::memory().unwrap());
    let skill = DiscoverableSkill {
        key: "owner/repo:new-skill".into(),
        name: "new".into(),
        description: String::new(),
        directory: "new-skill".into(),
        repo_owner: "owner".into(),
        repo_name: "repo".into(),
        repo_branch: "main".into(),
        readme_url: None,
    };
    let (installed, migrated) =
        tokio::join!(service.install(&db, &skill, &AppType::Claude), async {
            SkillService::migrate_storage(&db, SkillStorageLocation::Unified)
        });
    assert!(migrated.unwrap().location_changed);
    assert!(installed.is_ok(), "{installed:?}");
    assert!(home
        .path()
        .join(".agents/skills/new-skill/SKILL.md")
        .is_file());
    assert!(!home.path().join(".cc-switch/skills/new-skill").exists());
}

#[tokio::test]
#[serial_test::serial]
async fn update_after_migration_refreshes_active_source_and_native_copy() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let remote = tempdir().unwrap();
    write_skill(&remote.path().join("skill"), "new");
    let service = SkillService {
        repo_fixture: Some(remote.path().into()),
        ..SkillService::new()
    };
    let db = Arc::new(Database::memory().unwrap());
    let skill = repo_skill("skill");
    db.save_skill(&skill).unwrap();
    let old = SkillService::get_ssot_dir().unwrap().join("skill");
    let native = SkillService::get_app_skills_dir(&AppType::Claude)
        .unwrap()
        .join("skill");
    write_skill(&old, "old");
    SkillService::copy_dir_recursive(&old, &native).unwrap();
    let (updated, migrated) = tokio::join!(service.update_skill(&db, &skill.id), async {
        SkillService::migrate_storage(&db, SkillStorageLocation::Unified)
    });
    assert!(migrated.unwrap().location_changed);
    assert!(updated.is_ok(), "{updated:?}");
    assert!(!old.exists());
    assert!(fs::read_to_string(native.join("SKILL.md"))
        .unwrap()
        .contains("name: new"));
    assert_eq!(
        SkillService::compute_dir_hash(&native).unwrap(),
        SkillService::compute_dir_hash(&SkillService::get_ssot_dir().unwrap().join("skill"))
            .unwrap()
    );
}

#[tokio::test]
#[serial_test::serial]
async fn update_database_failure_restores_ssot_and_claude_copy() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let remote = tempdir().unwrap();
    write_skill(&remote.path().join("skill"), "new");
    let service = SkillService {
        repo_fixture: Some(remote.path().into()),
        ..SkillService::new()
    };
    let db = Arc::new(Database::memory().unwrap());
    let skill = repo_skill("skill");
    db.save_skill(&skill).unwrap();
    let ssot = SkillService::get_ssot_dir().unwrap().join("skill");
    let native = SkillService::get_app_skills_dir(&AppType::Claude)
        .unwrap()
        .join("skill");
    write_skill(&ssot, "old");
    SkillService::copy_dir_recursive(&ssot, &native).unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only = ON")
        .unwrap();
    assert!(service.update_skill(&db, &skill.id).await.is_err());
    assert!(fs::read_to_string(ssot.join("SKILL.md"))
        .unwrap()
        .contains("name: old"));
    assert!(fs::read_to_string(native.join("SKILL.md"))
        .unwrap()
        .contains("name: old"));
    db.conn
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only = OFF")
        .unwrap();
    service.update_skill(&db, &skill.id).await.unwrap();
    assert!(fs::read_to_string(native.join("SKILL.md"))
        .unwrap()
        .contains("name: new"));
}

#[test]
#[serial_test::serial]
fn zip_database_failure_leaves_no_orphan_and_can_retry() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let db = Arc::new(Database::memory().unwrap());
    let zip = home.path().join("skill.zip");
    fs::write(&zip, build_skills_zip(&["skill"])).unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only = ON")
        .unwrap();
    assert!(SkillService::install_from_zip(&db, &zip, &AppType::Claude).is_err());
    assert!(!SkillService::get_ssot_dir().unwrap().join("skill").exists());
    assert!(!SkillService::get_app_skills_dir(&AppType::Claude)
        .unwrap()
        .join("skill")
        .exists());
    db.conn
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only = OFF")
        .unwrap();
    assert_eq!(
        SkillService::install_from_zip(&db, &zip, &AppType::Claude)
            .unwrap()
            .installed
            .len(),
        1
    );
}

#[test]
#[serial_test::serial]
fn uninstall_database_failure_restores_all_copies() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let db = Arc::new(Database::memory().unwrap());
    let skill = repo_skill("skill");
    db.save_skill(&skill).unwrap();
    let ssot = SkillService::get_ssot_dir().unwrap().join("skill");
    let native = SkillService::get_app_skills_dir(&AppType::Claude)
        .unwrap()
        .join("skill");
    write_skill(&ssot, "old");
    SkillService::copy_dir_recursive(&ssot, &native).unwrap();
    db.conn
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only = ON")
        .unwrap();
    assert!(SkillService::uninstall(&db, &skill.id).is_err());
    assert!(ssot.join("SKILL.md").is_file());
    assert!(native.join("SKILL.md").is_file());
    assert!(db.get_installed_skill(&skill.id).unwrap().is_some());
}

#[test]
#[serial_test::serial]
fn repeated_import_preserves_existing_id_metadata_and_enabled_apps() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let db = Arc::new(Database::memory().unwrap());
    let skill = repo_skill("skill");
    db.save_skill(&skill).unwrap();
    let result = SkillService::import_from_apps(
        &db,
        vec![
            ImportSkillSelection {
                directory: "skill".into(),
                apps: SkillApps::only(&AppType::Codex),
            },
            ImportSkillSelection {
                directory: "skill".into(),
                apps: SkillApps::default(),
            },
        ],
    )
    .unwrap();
    assert_eq!(result.len(), 1);
    assert_eq!(result[0].id, skill.id);
    assert_eq!(result[0].apps, skill.apps);
    assert_eq!(db.get_all_installed_skills().unwrap().len(), 1);
}

#[test]
fn portable_directory_validation_rejects_windows_aliases_and_streams() {
    for name in [
        "a:b", "NUL", "con.txt", "COM1", "LPT9.log", "COM¹", "skill.", "bad?", "bad\0",
    ] {
        assert!(
            SkillService::require_valid_directory(name).is_err(),
            "{name:?}"
        );
    }
    for name in ["console", "COM10", "my.skill", "技能"] {
        assert!(
            SkillService::require_valid_directory(name).is_ok(),
            "{name:?}"
        );
    }
}

#[test]
fn content_hash_does_not_follow_directory_links() {
    let root = tempdir().unwrap();
    write_skill(root.path(), "skill");
    let hash = SkillService::compute_dir_hash(root.path()).unwrap();
    let result = SkillService::create_symlink(root.path(), &root.path().join("cycle"));
    #[cfg(windows)]
    if result.is_err() {
        return;
    }
    result.unwrap();
    assert_eq!(SkillService::compute_dir_hash(root.path()).unwrap(), hash);
}

#[test]
#[serial_test::serial]
fn import_database_failure_preserves_native_source_and_removes_new_copies() {
    let home = tempdir().unwrap();
    let guard = TestHomeGuard::set(home.path());
    let _location = StorageLocationGuard::set(&guard, SkillStorageLocation::CcSwitch);
    let db = Arc::new(Database::memory().unwrap());
    let source = SkillService::get_app_skills_dir(&AppType::Claude)
        .unwrap()
        .join("skill");
    let codex = SkillService::get_app_skills_dir(&AppType::Codex)
        .unwrap()
        .join("skill");
    write_skill(&source, "native");
    db.conn
        .lock()
        .unwrap()
        .execute_batch("PRAGMA query_only = ON")
        .unwrap();
    let result = SkillService::import_from_apps(
        &db,
        vec![ImportSkillSelection {
            directory: "skill".into(),
            apps: SkillApps::only(&AppType::Codex),
        }],
    );
    assert!(result.is_err());
    assert!(source.join("SKILL.md").is_file());
    assert!(!codex.exists());
    assert!(!SkillService::get_ssot_dir().unwrap().join("skill").exists());
    assert!(db.get_all_installed_skills().unwrap().is_empty());
}

#[test]
fn metadata_delimiters_are_lines_and_must_start_the_document() {
    let temp = tempdir().unwrap();
    let path = temp.path().join("SKILL.md");
    fs::write(
        &path,
        "---\nname: skill\ndescription: 'before---after'\n---\nbody",
    )
    .unwrap();
    let meta = SkillService::parse_skill_metadata_static(&path).unwrap();
    assert_eq!(meta.description.as_deref(), Some("before---after"));
    fs::write(&path, "# Examples\n---\nname: not-front-matter\n---").unwrap();
    assert!(SkillService::parse_skill_metadata_static(&path)
        .unwrap()
        .name
        .is_none());
}

#[test]
fn update_rejects_ambiguous_same_name_directories() {
    let root = tempdir().unwrap();
    write_skill(&root.path().join("one/skill"), "one");
    write_skill(&root.path().join("two/skill"), "two");
    let repo = SkillRepo {
        owner: "owner".into(),
        name: "repo".into(),
        branch: "main".into(),
        enabled: true,
    };
    let mut remote = Vec::new();
    SkillService::new()
        .scan_dir_recursive(root.path(), root.path(), &repo, &mut remote)
        .unwrap();
    assert!(SkillService::find_remote_skill_for_install(&remote, "skill", None).is_none());
    assert_eq!(
        SkillService::find_remote_skill_for_install(
            &remote,
            "skill",
            Some("https://github.com/owner/repo/blob/main/two/skill/SKILL.md")
        )
        .unwrap()
        .directory,
        "two/skill"
    );
}

#[test]
fn nested_skill_roots_are_rejected() {
    let root = tempdir().unwrap();
    let ssot = root.path().join("skills");
    assert!(SkillService::ensure_distinct_skill_roots(
        &ssot,
        &ssot.join("native/skills"),
        &AppType::Claude
    )
    .is_err());
    assert!(SkillService::ensure_distinct_skill_roots(
        &ssot.join("nested"),
        &ssot,
        &AppType::Claude
    )
    .is_err());
}
