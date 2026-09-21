use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    #[serde(default = "default_worktree_root")]
    pub worktree_root: PathBuf,
    #[serde(default)]
    pub projects: BTreeMap<String, ProjectLink>,
    #[serde(default)]
    pub tasks: BTreeMap<u64, TaskLink>,
    #[serde(default = "default_profiles")]
    pub profiles: BTreeMap<String, AgentProfile>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProjectLink {
    pub path: PathBuf,
    pub repository_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TaskLink {
    pub project: String,
    pub branch: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AgentProfile {
    pub command: String,
    #[serde(default)]
    pub args: Vec<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            worktree_root: default_worktree_root(),
            projects: BTreeMap::new(),
            tasks: BTreeMap::new(),
            profiles: default_profiles(),
        }
    }
}

impl Config {
    pub fn path() -> Result<PathBuf> {
        if let Some(path) = std::env::var_os("AKH_CONFIG_HOME") {
            return Ok(PathBuf::from(path).join("config.toml"));
        }

        home_dir()
            .map(|path| path.join(".akh").join("config.toml"))
            .context("could not determine the user home directory")
    }

    pub fn load() -> Result<Self> {
        Self::load_from(&Self::path()?)
    }

    pub fn load_from(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::default());
        }
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        self.save_to(&Self::path()?)
    }

    pub fn save_to(&self, path: &Path) -> Result<()> {
        let parent = path.parent().context("configuration path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        let contents = toml::to_string_pretty(self).context("failed to serialize configuration")?;
        fs::write(path, contents).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn project(&self, name: &str) -> Result<&ProjectLink> {
        self.projects
            .get(name)
            .with_context(|| format!("project '{name}' is not linked; run `akh project link`"))
    }

    pub fn task(&self, id: u64) -> Result<&TaskLink> {
        self.tasks
            .get(&id)
            .with_context(|| format!("task {id} is not linked; run `akh task-link {id}`"))
    }

    pub fn profile(&self, name: &str) -> Result<&AgentProfile> {
        self.profiles
            .get(name)
            .with_context(|| format!("agent profile '{name}' is not configured"))
    }
}

fn default_profiles() -> BTreeMap<String, AgentProfile> {
    ["claude", "codex"]
        .into_iter()
        .map(|name| {
            (
                name.to_owned(),
                AgentProfile {
                    command: name.to_owned(),
                    args: Vec::new(),
                },
            )
        })
        .collect()
}

fn default_worktree_root() -> PathBuf {
    home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".akh")
        .join("worktrees")
}

fn home_dir() -> Option<PathBuf> {
    std::env::var_os("HOME")
        .or_else(|| std::env::var_os("USERPROFILE"))
        .map(PathBuf::from)
}

pub fn validate_name(name: &str, kind: &str) -> Result<()> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        bail!("invalid {kind} name '{name}'; use letters, numbers, '-' or '_'")
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn config_round_trip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("config.toml");
        let mut config = Config::default();
        config.projects.insert(
            "akh".into(),
            ProjectLink {
                path: PathBuf::from("/tmp/akh"),
                repository_url: Some("https://github.com/cccadet/akh.git".into()),
            },
        );
        config.save_to(&path).unwrap();
        let loaded = Config::load_from(&path).unwrap();
        assert_eq!(
            loaded.projects["akh"].repository_url.as_deref(),
            Some("https://github.com/cccadet/akh.git")
        );
    }

    #[test]
    fn rejects_unsafe_names() {
        assert!(validate_name("../outside", "project").is_err());
        assert!(validate_name("akh", "project").is_ok());
    }
}
