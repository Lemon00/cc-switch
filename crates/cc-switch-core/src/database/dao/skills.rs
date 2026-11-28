//! Skills 数据访问对象
//!
//! 提供 Skills 和 Skill Repos 的 CRUD 操作。
//!
//! v3.10.0+ 统一管理架构：
//! - Skills 使用统一的 id 主键，按受支持应用保存启用标志
//! - 实际文件存储在 ~/.cc-switch/skills/，同步到各应用目录

use crate::app_config::{InstalledSkill, SkillApps};
use crate::database::{lock_conn, Database};
use crate::error::AppError;
use crate::skill_model::SkillRepo;
use indexmap::IndexMap;
use rusqlite::params;

impl Database {
    // ========== InstalledSkill CRUD ==========

    /// 获取所有已安装的 Skills
    pub fn get_all_installed_skills(&self) -> Result<IndexMap<String, InstalledSkill>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, directory, repo_owner, repo_name, repo_branch,
                        readme_url, enabled_claude, enabled_codex, enabled_opencode,
                        installed_at, content_hash, updated_at, enabled_agy, enabled_dsh
                 FROM skills ORDER BY name ASC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let skill_iter = stmt
            .query_map([], |row| {
                Ok(InstalledSkill {
                    id: row.get(0)?,
                    name: row.get(1)?,
                    description: row.get(2)?,
                    directory: row.get(3)?,
                    repo_owner: row.get(4)?,
                    repo_name: row.get(5)?,
                    repo_branch: row.get(6)?,
                    readme_url: row.get(7)?,
                    apps: SkillApps {
                        claude: row.get(8)?,
                        codex: row.get(9)?,
                        opencode: row.get(10)?,
                        agy: row.get(14)?,
                        dsh: row.get(15)?,
                    },
                    installed_at: row.get(11)?,
                    content_hash: row.get(12)?,
                    updated_at: row.get(13)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut skills = IndexMap::new();
        for skill_res in skill_iter {
            let skill = skill_res.map_err(|e| AppError::Database(e.to_string()))?;
            skills.insert(skill.id.clone(), skill);
        }
        Ok(skills)
    }

    /// 获取单个已安装的 Skill
    pub fn get_installed_skill(&self, id: &str) -> Result<Option<InstalledSkill>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT id, name, description, directory, repo_owner, repo_name, repo_branch,
                        readme_url, enabled_claude, enabled_codex, enabled_opencode,
                        installed_at, content_hash, updated_at, enabled_agy, enabled_dsh
                 FROM skills WHERE id = ?1",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let result = stmt.query_row([id], |row| {
            Ok(InstalledSkill {
                id: row.get(0)?,
                name: row.get(1)?,
                description: row.get(2)?,
                directory: row.get(3)?,
                repo_owner: row.get(4)?,
                repo_name: row.get(5)?,
                repo_branch: row.get(6)?,
                readme_url: row.get(7)?,
                apps: SkillApps {
                    claude: row.get(8)?,
                    codex: row.get(9)?,
                    opencode: row.get(10)?,
                    agy: row.get(14)?,
                    dsh: row.get(15)?,
                },
                installed_at: row.get(11)?,
                content_hash: row.get(12)?,
                updated_at: row.get(13)?,
            })
        });

        match result {
            Ok(skill) => Ok(Some(skill)),
            Err(rusqlite::Error::QueryReturnedNoRows) => Ok(None),
            Err(e) => Err(AppError::Database(e.to_string())),
        }
    }

    /// 保存 Skill（添加或更新）
    pub fn save_skill(&self, skill: &InstalledSkill) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        Self::save_skill_on(&conn, skill)
    }

    fn save_skill_on(conn: &rusqlite::Connection, skill: &InstalledSkill) -> Result<(), AppError> {
        conn.execute(
            "INSERT INTO skills (id, name, description, directory, repo_owner, repo_name, repo_branch, readme_url, enabled_claude, enabled_codex, enabled_opencode, installed_at, content_hash, updated_at, enabled_agy, enabled_dsh) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?15, ?16) ON CONFLICT(id) DO UPDATE SET name=excluded.name, description=excluded.description, directory=excluded.directory, repo_owner=excluded.repo_owner, repo_name=excluded.repo_name, repo_branch=excluded.repo_branch, readme_url=excluded.readme_url, enabled_claude=excluded.enabled_claude, enabled_codex=excluded.enabled_codex, enabled_opencode=excluded.enabled_opencode, installed_at=excluded.installed_at, content_hash=excluded.content_hash, updated_at=excluded.updated_at, enabled_agy=excluded.enabled_agy, enabled_dsh=excluded.enabled_dsh",
            params![
                skill.id,
                skill.name,
                skill.description,
                skill.directory,
                skill.repo_owner,
                skill.repo_name,
                skill.repo_branch,
                skill.readme_url,
                skill.apps.claude,
                skill.apps.codex,
                skill.apps.opencode,
                skill.installed_at,
                skill.content_hash,
                skill.updated_at,
                skill.apps.agy,
                skill.apps.dsh,
            ],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 原子提交迁移结果和完成标记；任一插入失败均保留旧表与重试快照。
    pub fn replace_skills_for_migration(&self, skills: &[InstalledSkill]) -> Result<(), AppError> {
        let mut conn = lock_conn!(self.conn);
        let tx = conn
            .transaction()
            .map_err(|e| AppError::Database(e.to_string()))?;
        tx.execute("DELETE FROM skills", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        for skill in skills {
            Self::save_skill_on(&tx, skill)?;
        }
        tx.execute("INSERT OR REPLACE INTO settings (key, value) VALUES ('skills_ssot_migration_snapshot', ''), ('skills_ssot_migration_pending', 'false')", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        tx.commit().map_err(|e| AppError::Database(e.to_string()))
    }

    /// 仅更新已安装 Skill 的元数据，不修改各应用的启用状态。
    ///
    /// 与 [`Self::save_skill`] 不同，本方法不会插入缺失记录。更新操作可能在网络
    /// 下载期间与启用状态切换或卸载并发发生，因此调用方必须保留数据库中的
    /// `enabled_*` 字段，并在记录已被删除时停止后续处理。
    pub fn update_skill_metadata(&self, skill: &InstalledSkill) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "UPDATE skills
                 SET name = ?1,
                     description = ?2,
                     directory = ?3,
                     repo_owner = ?4,
                     repo_name = ?5,
                     repo_branch = ?6,
                     readme_url = ?7,
                     installed_at = ?8,
                     content_hash = ?9,
                     updated_at = ?10
                 WHERE id = ?11 AND installed_at = ?12",
                params![
                    skill.name,
                    skill.description,
                    skill.directory,
                    skill.repo_owner,
                    skill.repo_name,
                    skill.repo_branch,
                    skill.readme_url,
                    skill.installed_at,
                    skill.content_hash,
                    skill.updated_at,
                    skill.id,
                    skill.installed_at,
                ],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// 删除 Skill
    pub fn delete_skill(&self, id: &str) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute("DELETE FROM skills WHERE id = ?1", params![id])
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// 清空所有 Skills（用于迁移）
    pub fn clear_skills(&self) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute("DELETE FROM skills", [])
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 更新 Skill 的应用启用状态
    pub fn update_skill_apps(&self, id: &str, apps: &SkillApps) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "UPDATE skills SET enabled_claude = ?1, enabled_codex = ?2, enabled_opencode = ?3, enabled_agy = ?4, enabled_dsh = ?5 WHERE id = ?6",
                params![apps.claude, apps.codex, apps.opencode, apps.agy, apps.dsh, id],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    /// 更新 Skill 的内容哈希和更新时间
    pub fn update_skill_hash(
        &self,
        id: &str,
        content_hash: &str,
        updated_at: i64,
    ) -> Result<bool, AppError> {
        let conn = lock_conn!(self.conn);
        let affected = conn
            .execute(
                "UPDATE skills SET content_hash = ?1, updated_at = ?2 WHERE id = ?3",
                params![content_hash, updated_at, id],
            )
            .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(affected > 0)
    }

    // ========== SkillRepo CRUD（保持原有） ==========

    /// 获取所有 Skill 仓库
    pub fn get_skill_repos(&self) -> Result<Vec<SkillRepo>, AppError> {
        let conn = lock_conn!(self.conn);
        let mut stmt = conn
            .prepare(
                "SELECT owner, name, branch, enabled FROM skill_repos ORDER BY owner ASC, name ASC",
            )
            .map_err(|e| AppError::Database(e.to_string()))?;

        let repo_iter = stmt
            .query_map([], |row| {
                Ok(SkillRepo {
                    owner: row.get(0)?,
                    name: row.get(1)?,
                    branch: row.get(2)?,
                    enabled: row.get(3)?,
                })
            })
            .map_err(|e| AppError::Database(e.to_string()))?;

        let mut repos = Vec::new();
        for repo_res in repo_iter {
            repos.push(repo_res.map_err(|e| AppError::Database(e.to_string()))?);
        }
        Ok(repos)
    }

    /// 保存 Skill 仓库
    pub fn save_skill_repo(&self, repo: &SkillRepo) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "INSERT OR REPLACE INTO skill_repos (owner, name, branch, enabled) VALUES (?1, ?2, ?3, ?4)",
            params![repo.owner, repo.name, repo.branch, repo.enabled],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 删除 Skill 仓库
    pub fn delete_skill_repo(&self, owner: &str, name: &str) -> Result<(), AppError> {
        let conn = lock_conn!(self.conn);
        conn.execute(
            "DELETE FROM skill_repos WHERE owner = ?1 AND name = ?2",
            params![owner, name],
        )
        .map_err(|e| AppError::Database(e.to_string()))?;
        Ok(())
    }

    /// 初始化默认的 Skill 仓库（启动时调用，每个数据库仅执行一次）
    pub fn init_default_skill_repos(&self) -> Result<usize, AppError> {
        const INITIALIZED_KEY: &str = "default_skill_repos_initialized";
        let mut conn = lock_conn!(self.conn);
        let tx = conn.transaction_with_behavior(rusqlite::TransactionBehavior::Immediate)?;
        let initialized: bool = tx.query_row(
            "SELECT EXISTS(SELECT 1 FROM settings WHERE key = ?1 AND value IN ('true', '1'))",
            [INITIALIZED_KEY],
            |row| row.get(0),
        )?;
        if initialized {
            return Ok(0);
        }
        let has_repos: bool =
            tx.query_row("SELECT EXISTS(SELECT 1 FROM skill_repos)", [], |row| {
                row.get(0)
            })?;
        let mut count = 0;
        if !has_repos {
            for repo in crate::skill_model::SkillStore::default().repos {
                count += tx.execute(
                    "INSERT INTO skill_repos (owner, name, branch, enabled) VALUES (?1, ?2, ?3, ?4)",
                    params![repo.owner, repo.name, repo.branch, repo.enabled],
                )?;
            }
        }
        tx.execute(
            "INSERT OR REPLACE INTO settings (key, value) VALUES (?1, 'true')",
            [INITIALIZED_KEY],
        )?;
        tx.commit()?;
        Ok(count)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::app_config::AppType;

    fn skill(id: &str, name: &str, apps: SkillApps) -> InstalledSkill {
        InstalledSkill {
            id: id.to_string(),
            name: name.to_string(),
            description: Some(format!("{name} description")),
            directory: format!("{name}-directory"),
            repo_owner: Some("owner".to_string()),
            repo_name: Some("repo".to_string()),
            repo_branch: Some("main".to_string()),
            readme_url: Some(format!("https://example.com/{name}")),
            apps,
            installed_at: 1,
            content_hash: Some(format!("{name}-hash")),
            updated_at: 2,
        }
    }

    #[test]
    fn update_skill_metadata_preserves_enabled_apps() {
        let db = Database::memory().expect("memory db");
        let installed_apps = SkillApps::only(&AppType::Codex);
        let original = skill("owner/repo:skill", "original", installed_apps.clone());
        db.save_skill(&original).expect("seed skill");

        let mut candidate = skill(&original.id, "updated", SkillApps::only(&AppType::Claude));
        candidate.repo_branch = Some("next".to_string());
        candidate.updated_at = 42;

        assert!(db
            .update_skill_metadata(&candidate)
            .expect("update metadata"));

        let stored = db
            .get_installed_skill(&original.id)
            .expect("query skill")
            .expect("skill remains installed");
        assert_eq!(stored.name, candidate.name);
        assert_eq!(stored.description, candidate.description);
        assert_eq!(stored.directory, candidate.directory);
        assert_eq!(stored.repo_branch, candidate.repo_branch);
        assert_eq!(stored.readme_url, candidate.readme_url);
        assert_eq!(stored.content_hash, candidate.content_hash);
        assert_eq!(stored.updated_at, candidate.updated_at);
        assert_eq!(stored.apps, installed_apps);
    }

    #[test]
    fn update_skill_metadata_does_not_insert_missing_skill() {
        let db = Database::memory().expect("memory db");
        let candidate = skill(
            "owner/repo:missing",
            "missing",
            SkillApps::only(&AppType::Claude),
        );

        assert!(!db
            .update_skill_metadata(&candidate)
            .expect("missing update is not an error"));
        assert!(db
            .get_installed_skill(&candidate.id)
            .expect("query skill")
            .is_none());
    }
}
