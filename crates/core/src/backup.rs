use std::{
    collections::BTreeMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};

use crate::{AgentProfile, Config, Conversation, ProjectLink, TaskLink};

const FORMAT_VERSION: u32 = 1;

#[derive(Debug, Serialize, Deserialize)]
struct Backup {
    format_version: u32,
    projects: BTreeMap<String, BackupProject>,
    tasks: BTreeMap<u64, TaskLink>,
    profiles: BTreeMap<String, AgentProfile>,
    conversations: Vec<Conversation>,
    #[serde(default)]
    sqlite_database: Option<Vec<u8>>,
}

#[derive(Debug, Serialize, Deserialize)]
struct BackupProject {
    repository_url: Option<String>,
    remote_id: Option<uuid::Uuid>,
}

#[derive(Debug, Clone, Copy, Serialize)]
pub struct BackupSummary {
    pub projects: usize,
    pub tasks: usize,
    pub conversations: usize,
    pub database_included: bool,
    pub restart_required: bool,
}

pub fn export_backup(destination: &Path, database: Option<&Path>) -> Result<BackupSummary> {
    let config = Config::load()?;
    let projects = config
        .projects
        .iter()
        .map(|(name, project)| {
            (
                name.clone(),
                BackupProject {
                    repository_url: project.repository_url.clone(),
                    remote_id: project.remote_id,
                },
            )
        })
        .collect();
    let conversations = config
        .tasks
        .keys()
        .map(|id| Conversation::load(*id))
        .collect::<Result<Vec<_>>>()?;
    let sqlite_database = database
        .filter(|path| path.exists())
        .map(fs::read)
        .transpose()?;
    let backup = Backup {
        format_version: FORMAT_VERSION,
        projects,
        tasks: config.tasks,
        profiles: config.profiles,
        conversations,
        sqlite_database,
    };
    if let Some(parent) = destination
        .parent()
        .filter(|path| !path.as_os_str().is_empty())
    {
        fs::create_dir_all(parent)?;
    }
    fs::write(destination, serde_json::to_vec_pretty(&backup)?)
        .with_context(|| format!("failed to write {}", destination.display()))?;
    Ok(summary(&backup, false))
}

pub fn import_backup(source: &Path, database: Option<&Path>) -> Result<BackupSummary> {
    let bytes = fs::read(source).with_context(|| format!("failed to read {}", source.display()))?;
    let backup: Backup = serde_json::from_slice(&bytes).context("invalid Akh backup")?;
    if backup.format_version != FORMAT_VERSION {
        bail!("unsupported Akh backup version {}", backup.format_version);
    }

    let mut config = Config::load()?;
    config.tasks = backup.tasks.clone();
    config.profiles = backup.profiles.clone();
    for (name, project) in &backup.projects {
        let existing_path = config
            .projects
            .get(name)
            .map(|item| item.path.clone())
            .unwrap_or_default();
        config.projects.insert(
            name.clone(),
            ProjectLink {
                path: existing_path,
                repository_url: project.repository_url.clone(),
                remote_id: project.remote_id,
            },
        );
    }
    config.save()?;
    for conversation in &backup.conversations {
        conversation.save()?;
    }

    let restart_required =
        if let (Some(contents), Some(database)) = (&backup.sqlite_database, database) {
            let staged = staged_database_path(database);
            if let Some(parent) = staged.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(staged, contents)?;
            true
        } else {
            false
        };
    Ok(summary(&backup, restart_required))
}

pub fn apply_staged_database(database: &Path) -> Result<bool> {
    let staged = staged_database_path(database);
    if !staged.exists() {
        return Ok(false);
    }
    if database.exists() {
        fs::remove_file(database)?;
    }
    fs::rename(staged, database)?;
    Ok(true)
}

fn staged_database_path(database: &Path) -> PathBuf {
    database.with_extension("db.import")
}

fn summary(backup: &Backup, restart_required: bool) -> BackupSummary {
    BackupSummary {
        projects: backup.projects.len(),
        tasks: backup.tasks.len(),
        conversations: backup.conversations.len(),
        database_included: backup.sqlite_database.is_some(),
        restart_required,
    }
}
