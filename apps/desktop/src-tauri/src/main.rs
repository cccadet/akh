use std::path::PathBuf;
use std::process::Command;

use akh_core::{Config, GitRepository, ProjectLink, TaskLink};
use serde::Serialize;
use sqlx::sqlite::SqliteConnectOptions;
use tauri::Manager;

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopState {
    server_configured: bool,
    worktree_root: String,
    terminal_command: String,
    projects: Vec<DesktopProject>,
    tasks: Vec<DesktopTask>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopProject {
    name: String,
    path: String,
    repository_url: Option<String>,
    synced: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopTask {
    id: u64,
    title: String,
    project: String,
    branch: String,
    latest_commit: Option<String>,
    last_agent: Option<String>,
    synced: bool,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct BackupResult {
    path: String,
    projects: usize,
    tasks: usize,
    conversations: usize,
    database_included: bool,
    restart_required: bool,
}

#[tauri::command]
fn get_state() -> Result<DesktopState, String> {
    let config = Config::load().map_err(to_string)?;
    Ok(DesktopState {
        server_configured: config.server.is_some() && config.token.is_some(),
        worktree_root: config.worktree_root.display().to_string(),
        terminal_command: default_terminal().into(),
        projects: config
            .projects
            .into_iter()
            .map(|(name, project)| DesktopProject {
                name,
                path: project.path.display().to_string(),
                repository_url: project.repository_url,
                synced: project.remote_id.is_some(),
            })
            .collect(),
        tasks: config
            .tasks
            .into_iter()
            .map(|(id, task)| {
                let last_agent = akh_core::Conversation::load(id)
                    .ok()
                    .and_then(|conversation| {
                        conversation
                            .messages
                            .last()
                            .map(|message| message.agent.clone())
                    });
                DesktopTask {
                    id,
                    title: task.title,
                    project: task.project,
                    branch: task.branch,
                    latest_commit: task.latest_commit,
                    last_agent,
                    synced: task.remote_id.is_some(),
                }
            })
            .collect(),
    })
}

#[tauri::command]
fn link_project(path: String, name: Option<String>) -> Result<(), String> {
    let repo = GitRepository::discover(&PathBuf::from(path)).map_err(to_string)?;
    let inferred = repo
        .root
        .file_name()
        .and_then(|part| part.to_str())
        .ok_or("could not infer project name")?;
    let name = name
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| inferred.into());
    akh_core::config::validate_name(&name, "project").map_err(to_string)?;
    let mut config = Config::load().map_err(to_string)?;
    config.projects.insert(
        name,
        ProjectLink {
            path: repo.root,
            repository_url: repo.origin,
            remote_id: None,
        },
    );
    config.save().map_err(to_string)
}

#[tauri::command]
fn create_task(project: String, title: String, branch: Option<String>) -> Result<u64, String> {
    akh_core::config::validate_name(&project, "project").map_err(to_string)?;
    let title = title.trim();
    if title.is_empty() {
        return Err("task title is required".into());
    }

    let mut config = Config::load().map_err(to_string)?;
    config.project(&project).map_err(to_string)?;
    let id = config.tasks.keys().next_back().copied().unwrap_or(0) + 1;
    let branch = branch
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| format!("task/{id}"));
    GitRepository::validate_branch(&branch).map_err(to_string)?;
    config.tasks.insert(
        id,
        TaskLink {
            project,
            branch,
            title: title.into(),
            latest_commit: None,
            remote_id: None,
        },
    );
    config.save().map_err(to_string)?;
    Ok(id)
}

#[tauri::command]
fn launch_task(id: u64, agent: String) -> Result<(), String> {
    akh_core::config::validate_name(&agent, "agent").map_err(to_string)?;
    let config = Config::load().map_err(to_string)?;
    config.task(id).map_err(to_string)?;
    #[cfg(windows)]
    let mut command = {
        let mut cmd = Command::new("wt.exe");
        cmd.args(["akh", "task", &id.to_string(), &agent]);
        cmd
    };
    #[cfg(target_os = "macos")]
    let mut command = {
        let mut cmd = Command::new("open");
        cmd.args(["-a", "Terminal", "akh", "task", &id.to_string(), &agent]);
        cmd
    };
    #[cfg(all(unix, not(target_os = "macos")))]
    let mut command = {
        let mut cmd = Command::new("x-terminal-emulator");
        cmd.args(["-e", "akh", "task", &id.to_string(), &agent]);
        cmd
    };
    command.spawn().map_err(to_string)?;
    Ok(())
}

#[tauri::command]
fn handoff_task(id: u64, to: String, note: String) -> Result<(), String> {
    let config = Config::load().map_err(to_string)?;
    let task = config.task(id).map_err(to_string)?;
    let remote_id = task.remote_id.ok_or("task is not synchronized")?;
    let server = config.server.as_deref().ok_or("server is not configured")?;
    let token = config.token.as_deref().ok_or("token is not configured")?;
    let recipient = to.parse().map_err(|_| "recipient must be a user UUID")?;
    akh_client::ServerClient::new(server, Some(token))
        .map_err(to_string)?
        .create_handoff(remote_id, recipient, &note)
        .map_err(to_string)?;
    Ok(())
}

#[tauri::command]
async fn export_backup(app: tauri::AppHandle) -> Result<Option<BackupResult>, String> {
    let Some(path) = rfd::FileDialog::new()
        .set_file_name("akh-backup.akh-backup")
        .add_filter("Akh backup", &["akh-backup"])
        .save_file()
    else {
        return Ok(None);
    };
    let database = app.path().app_data_dir().map_err(to_string)?.join("akh.db");
    let snapshot = database.with_extension("backup-snapshot.db");
    akh_local_server::sanitized_snapshot(&database, &snapshot)
        .await
        .map_err(to_string)?;
    let export = akh_core::export_backup(&path, Some(&snapshot));
    std::fs::remove_file(snapshot).map_err(to_string)?;
    let summary = export.map_err(to_string)?;
    Ok(Some(backup_result(path, summary)))
}

#[tauri::command]
fn import_backup(app: tauri::AppHandle) -> Result<Option<BackupResult>, String> {
    let Some(path) = rfd::FileDialog::new()
        .add_filter("Akh backup", &["akh-backup"])
        .pick_file()
    else {
        return Ok(None);
    };
    let database = app.path().app_data_dir().map_err(to_string)?.join("akh.db");
    let summary = akh_core::import_backup(&path, Some(&database)).map_err(to_string)?;
    Ok(Some(backup_result(path, summary)))
}

fn backup_result(path: PathBuf, summary: akh_core::BackupSummary) -> BackupResult {
    BackupResult {
        path: path.display().to_string(),
        projects: summary.projects,
        tasks: summary.tasks,
        conversations: summary.conversations,
        database_included: summary.database_included,
        restart_required: summary.restart_required,
    }
}

fn default_terminal() -> &'static str {
    if cfg!(windows) {
        "Windows Terminal"
    } else if cfg!(target_os = "macos") {
        "Terminal.app"
    } else {
        "System terminal"
    }
}

fn to_string(error: impl std::fmt::Display) -> String {
    error.to_string()
}

fn main() {
    tauri::Builder::default()
        .setup(|app| {
            let data_dir = app.path().app_data_dir()?;
            std::fs::create_dir_all(&data_dir)?;
            let database = data_dir.join("akh.db");
            akh_core::backup::apply_staged_database(&database)?;
            let options = SqliteConnectOptions::new().filename(database);
            let address = "127.0.0.1:3000".parse()?;
            tauri::async_runtime::spawn(async move {
                if let Err(error) = akh_local_server::serve(options, address).await {
                    eprintln!("embedded Akh server failed: {error:#}");
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            get_state,
            link_project,
            create_task,
            launch_task,
            handoff_task,
            export_backup,
            import_backup
        ])
        .run(tauri::generate_context!())
        .expect("error while running Akh desktop");
}
