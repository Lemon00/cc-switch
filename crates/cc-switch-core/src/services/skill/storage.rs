//! Storage migration keeps the active tree intact until files, projections and
//! the location setting can all be committed. Failed commits restore old paths.

use super::*;

impl SkillService {
    pub fn migrate_storage(
        db: &Arc<Database>,
        target: SkillStorageLocation,
    ) -> Result<MigrationResult> {
        let _state_guard = skill_state_write_guard();
        let mut result = MigrationResult {
            migrated_count: 0,
            skipped_count: 0,
            errors: Vec::new(),
            location_changed: false,
        };
        if crate::settings::get_skill_storage_location() == target {
            return Ok(result);
        }

        let old_dir = Self::get_ssot_dir()?;
        let new_dir = match target {
            SkillStorageLocation::CcSwitch => get_app_config_dir().join("skills"),
            SkillStorageLocation::Unified => crate::config::get_home_dir().join(".agents/skills"),
        };
        fs::create_dir_all(&new_dir)?;
        Self::validate_skill_storage_destination(&new_dir)?;
        if Self::paths_overlap(&old_dir, &new_dir) {
            return Err(anyhow!("Skill storage roots overlap"));
        }

        let skills = db.get_all_installed_skills()?;
        let mut staged = Vec::new();
        let mut old_copies = Vec::new();
        let mut migrated_count = 0;
        let mut seen = HashSet::new();
        for skill in skills.values() {
            let prepared = (|| -> Result<()> {
                let directory = Self::require_valid_directory(&skill.directory)?;
                if !seen.insert(directory.to_lowercase()) {
                    return Err(anyhow!(
                        "Multiple installed Skills own directory {directory}"
                    ));
                }
                let source = old_dir.join(&directory);
                let destination = new_dir.join(&directory);
                // Missing files require repair through update/restore; switching
                // to an unrelated same-name target must never silently adopt it.
                Self::validate_sync_source_dir(&source, &directory)?;
                let source_hash = Self::compute_dir_hash(&source)?;
                if destination.exists() || Self::is_symlink(&destination) {
                    if Self::is_symlink(&destination)
                        || !destination.is_dir()
                        || Self::compute_dir_hash(&destination)? != source_hash
                    {
                        return Err(anyhow!(
                            "Target Skill directory has different contents: {}",
                            destination.display()
                        ));
                    }
                    result.skipped_count += 1;
                } else {
                    let stage = Self::stage_skill_copy(&source, &destination)?;
                    if Self::compute_dir_hash(&stage.path().join("new"))? != source_hash {
                        return Err(anyhow!("Skill could not be copied completely: {directory}"));
                    }
                    staged.push((destination.clone(), stage));
                    migrated_count += 1;
                }

                let mut source_is_native = false;
                for app in AppType::persisted() {
                    if !skill.apps.is_enabled_for(&app) {
                        continue;
                    }
                    let native = Self::get_app_skills_dir(&app)?.join(&directory);
                    if staged
                        .iter()
                        .any(|(path, _)| Self::same_path_entry(path, &native))
                    {
                        continue;
                    }
                    let aliases_source = Self::same_path_entry(&source, &native);
                    let deployment = Self::inspect_skill_destination(&source, &native, &directory)?;

                    if !aliases_source && matches!(deployment, Some(SkillDeployment::Copy)) {
                        continue; // Contents are unchanged; only links need retargeting.
                    }
                    source_is_native |= aliases_source;
                    let parent = native.parent().context("Skill destination has no parent")?;
                    fs::create_dir_all(parent)?;
                    let stage = tempfile::Builder::new()
                        .prefix(".skill-migration-")
                        .tempdir_in(parent)?;
                    let replacement = stage.path().join("new");
                    match Self::get_sync_method() {
                        SyncMethod::Copy => Self::copy_dir_recursive(&source, &replacement)?,
                        method => {
                            if let Err(error) = Self::create_symlink(&destination, &replacement) {
                                if method == SyncMethod::Symlink {
                                    return Err(error);
                                }
                                Self::copy_dir_recursive(&source, &replacement)?;
                            }
                        }
                    }
                    staged.push((native, stage));
                }
                if !source_is_native {
                    old_copies.push(source);
                }
                Ok(())
            })();
            if let Err(error) = prepared {
                result
                    .errors
                    .push(format!("{}: {error:#}", skill.directory.escape_debug()));
            }
        }
        if !result.errors.is_empty() {
            return Ok(result); // TempDir drops every prepared copy; old tree is intact.
        }

        Self::commit_staged_skill_paths(staged, || {
            crate::settings::set_skill_storage_location(target)?;
            Ok(())
        })?;
        result.location_changed = true;
        result.migrated_count = migrated_count;
        // Cleanup is last. A failure leaves a redundant old copy, never a missing
        // active source or a dangling managed link.
        for source in old_copies {
            if let Err(error) = Self::remove_path(&source) {
                result
                    .errors
                    .push(format!("Cleanup {}: {error}", source.display()));
            }
        }
        Ok(result)
    }

    pub(super) fn stage_skill_copy(source: &Path, destination: &Path) -> Result<tempfile::TempDir> {
        let parent = destination
            .parent()
            .context("Skill destination has no parent")?;
        fs::create_dir_all(parent)?;
        let staging = tempfile::Builder::new()
            .prefix(".skill-storage-")
            .tempdir_in(parent)?;
        Self::copy_dir_recursive(source, &staging.path().join("new"))?;
        Ok(staging)
    }
}
