//! Import preparation and per-Skill commit. Existing native files are retained;
//! newly created SSOT/native copies are rolled back if persistence fails.
use super::*;

impl SkillService {
    pub fn import_from_apps(
        db: &Arc<Database>,
        imports: Vec<ImportSkillSelection>,
    ) -> Result<Vec<InstalledSkill>> {
        let _state_guard = skill_state_write_guard();
        let ssot = Self::get_ssot_dir()?;
        let lock = parse_agents_lock();
        save_repos_from_lock(
            db,
            &lock,
            imports.iter().map(|item| item.directory.as_str()),
        );
        let mut sources: Vec<_> = AppType::all()
            .filter_map(|app| Self::get_app_skills_dir(&app).ok())
            .collect();
        if let Some(agents) = get_agents_skills_dir() {
            sources.push(agents);
        }
        sources.push(ssot.clone());
        let mut installed = db.get_all_installed_skills()?;
        let mut imported = Vec::new();
        let mut problems = Vec::new();

        'imports: for selection in imports {
            let directory = match Self::require_valid_directory(&selection.directory) {
                Ok(directory) => directory,
                Err(error) => {
                    problems.push(error.to_string());
                    continue;
                }
            };
            if let Some(existing) = installed
                .values()
                .find(|s| s.directory.eq_ignore_ascii_case(&directory))
            {
                if !imported
                    .iter()
                    .any(|s: &InstalledSkill| s.id == existing.id)
                {
                    imported.push(existing.clone());
                }
                continue;
            }
            let destination = ssot.join(&directory);
            let source = if destination.join("SKILL.md").is_file() {
                destination.clone()
            } else {
                let Some(source) = sources
                    .iter()
                    .map(|base| base.join(&directory))
                    .find(|path| path.join("SKILL.md").is_file())
                else {
                    problems.push(format!("Skill source missing: {directory}"));
                    continue;
                };
                source
            };
            let mut staged = Vec::new();
            if !Self::same_path_entry(&source, &destination) {
                if destination.exists() || Self::is_symlink(&destination) {
                    problems.push(format!("Invalid existing SSOT directory: {directory}"));
                    continue;
                }
                match Self::stage_skill_copy(&source, &destination) {
                    Ok(stage) => staged.push((destination.clone(), stage)),
                    Err(error) => {
                        problems.push(format!("{directory}: copy failed: {error:#}"));
                        continue;
                    }
                }
            }
            let mut apps = selection.apps;

            for app in apps.enabled_apps() {
                let strict = matches!(app, AppType::Agy | AppType::Dsh);
                let prepared = (|| -> Result<()> {
                    let native = Self::get_distinct_app_skills_dir(&ssot, &app)?.join(&directory);
                    if staged
                        .iter()
                        .any(|(path, _)| Self::same_path_entry(path, &native))
                    {
                        return Ok(());
                    }
                    let exists = native.exists() || Self::is_symlink(&native);
                    if exists {
                        if !strict {
                            return Ok(());
                        }
                        if Self::compute_dir_hash(&native)? != Self::compute_dir_hash(&source)? {
                            return Err(anyhow!("Skill 与托管副本内容不同，拒绝替换: {directory}"));
                        }
                        if !Self::is_symlink(&native) || Self::paths_alias(&native, &destination) {
                            return Ok(());
                        }
                    }
                    let parent = native.parent().context("Skill destination has no parent")?;
                    fs::create_dir_all(parent)?;
                    let stage = tempfile::Builder::new()
                        .prefix(".skill-import-")
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
                    Ok(())
                })();
                if let Err(error) = prepared {
                    problems.push(format!("{directory} ({}): {error:#}", app.as_str()));
                    if strict {
                        continue 'imports;
                    }
                    apps.set_enabled_for(&app, false);
                }
            }
            let (id, repo_owner, repo_name, repo_branch, readme_url) =
                build_repo_info_from_lock(&lock, &directory);
            if installed.contains_key(&id) {
                problems.push(format!("Skill ID already installed: {id}"));
                continue;
            }
            let (name, description) =
                Self::read_skill_name_desc(&source.join("SKILL.md"), &directory);
            let skill = InstalledSkill {
                id,
                name,
                description,
                directory: directory.clone(),
                repo_owner,
                repo_name,
                repo_branch,
                readme_url,
                apps,
                installed_at: Utc::now().timestamp(),
                content_hash: Self::compute_dir_hash(&source).ok(),
                updated_at: 0,
            };
            if let Err(error) = Self::commit_staged_skill_paths(staged, || {
                db.save_skill(&skill)?;
                Ok(())
            }) {
                problems.push(format!("{directory}: save failed: {error:#}"));
                continue;
            }
            installed.insert(skill.id.clone(), skill.clone());
            imported.push(skill);
        }
        if problems.is_empty() {
            Ok(imported)
        } else {
            Err(anyhow!(
                "Imported {} Skills; {}",
                imported.len(),
                problems.join("; ")
            ))
        }
    }
}
