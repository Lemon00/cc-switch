//! Shared skill storage models, independent of installation and network services.
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Skill 同步方式
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum SyncMethod {
    /// 自动选择：优先 symlink，失败时回退到 copy
    #[default]
    Auto,
    /// 符号链接（推荐，节省磁盘空间）
    Symlink,
    /// 文件复制（兼容模式）
    Copy,
}

/// Skill 存储位置（SSOT 目录选择）
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum SkillStorageLocation {
    /// CC Switch 管理目录 (~/.cc-switch/skills/)
    #[default]
    CcSwitch,
    /// Agent Skills 统一标准目录 (~/.agents/skills/)
    Unified,
}

/// 仓库配置
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillRepo {
    /// GitHub 用户/组织名
    pub owner: String,
    /// 仓库名称
    pub name: String,
    /// 分支 (默认 "main")
    pub branch: String,
    /// 是否启用
    pub enabled: bool,
}

/// 技能安装状态（旧版兼容）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillState {
    /// 是否已安装
    pub installed: bool,
    /// 安装时间
    #[serde(rename = "installedAt")]
    pub installed_at: DateTime<Utc>,
}

/// 持久化存储结构（仓库配置）
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SkillStore {
    /// directory -> 安装状态（旧版兼容，新版不使用）
    pub skills: HashMap<String, SkillState>,
    /// 仓库列表
    pub repos: Vec<SkillRepo>,
}

impl Default for SkillStore {
    fn default() -> Self {
        SkillStore {
            skills: HashMap::new(),
            repos: vec![
                SkillRepo {
                    owner: "anthropics".to_string(),
                    name: "skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "ComposioHQ".to_string(),
                    name: "awesome-claude-skills".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "cexll".to_string(),
                    name: "myclaude".to_string(),
                    branch: "master".to_string(),
                    enabled: true,
                },
                SkillRepo {
                    owner: "JimLiu".to_string(),
                    name: "baoyu-skills".to_string(),
                    branch: "main".to_string(),
                    enabled: true,
                },
            ],
        }
    }
}
