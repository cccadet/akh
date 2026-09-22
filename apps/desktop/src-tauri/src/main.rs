#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

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
    server: Option<String>,
    worktree_root: String,
    terminal_command: String,
    projects: Vec<DesktopProject>,
    tasks: Vec<DesktopTask>,
    profiles: Vec<DesktopProfile>,
    users: Vec<DesktopUser>,
    notifications: Vec<DesktopNotification>,
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
    dirty: bool,
    commit_pushed: Option<bool>,
    messages: Vec<DesktopMessage>,
    pending_handoffs: Vec<DesktopHandoff>,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopMessage {
    role: String,
    agent: String,
    content: String,
    created_at: u64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopHandoff {
    id: String,
    from_user_id: String,
    note: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopProfile {
    name: String,
    command: String,
    args: Vec<String>,
}

#[derive(Serialize)]
struct DesktopUser {
    id: String,
    username: String,
    email: String,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct DesktopNotification {
    id: String,
    task_id: Option<i64>,
    kind: String,
    content: String,
    created_at: String,
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
    let linked_projects = config.projects.clone();
    let worktree_root = config.worktree_root.clone();
    let team_client = match (config.server.as_deref(), config.token.as_deref()) {
        (Some(server), Some(token)) => akh_client::ServerClient::new(server, Some(token)).ok(),
        _ => None,
    };
    let users = team_client
        .as_ref()
        .and_then(|client| client.users().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|user| DesktopUser {
            id: user.id.to_string(),
            username: user.username,
            email: user.email,
        })
        .collect();
    let notifications = team_client
        .as_ref()
        .and_then(|client| client.notifications().ok())
        .unwrap_or_default()
        .into_iter()
        .map(|notification| DesktopNotification {
            id: notification.id.to_string(),
            task_id: notification.task_id,
            kind: notification.kind,
            content: notification.content,
            created_at: notification.created_at.to_rfc3339(),
        })
        .collect();
    Ok(DesktopState {
        server_configured: config.server.is_some() && config.token.is_some(),
        server: config.server.clone(),
        worktree_root: config.worktree_root.display().to_string(),
        terminal_command: format!(
            "{} {}",
            config.terminal.command,
            config.terminal.args.join(" ")
        )
        .trim()
        .into(),
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
                let worktree = worktree_root.join(&task.project).join(id.to_string());
                let repository_path = if worktree.exists() {
                    Some(worktree)
                } else {
                    linked_projects
                        .get(&task.project)
                        .map(|project| project.path.clone())
                };
                let repository = repository_path
                    .as_deref()
                    .and_then(|path| GitRepository::discover(path).ok());
                let dirty = repository
                    .as_ref()
                    .and_then(|repo| repo.is_clean().ok())
                    .is_some_and(|clean| !clean);
                let commit_pushed = task.latest_commit.as_deref().and_then(|commit| {
                    repository
                        .as_ref()
                        .and_then(|repo| repo.commit_is_pushed(commit, &task.branch).ok())
                });
                let conversation = akh_core::Conversation::load(id).unwrap_or_default();
                let pending_handoffs = task
                    .remote_id
                    .and_then(|remote_id| {
                        team_client
                            .as_ref()
                            .and_then(|client| client.handoffs(remote_id).ok())
                    })
                    .unwrap_or_default()
                    .into_iter()
                    .filter(|handoff| {
                        handoff.accepted_at.is_none() && Some(handoff.to_user_id) == config.user_id
                    })
                    .map(|handoff| DesktopHandoff {
                        id: handoff.id.to_string(),
                        from_user_id: handoff.from_user_id.to_string(),
                        note: handoff.note,
                    })
                    .collect();
                let last_agent = conversation
                    .messages
                    .last()
                    .map(|message| message.agent.clone());
                DesktopTask {
                    id,
                    title: task.title,
                    project: task.project,
                    branch: task.branch,
                    latest_commit: task.latest_commit,
                    last_agent,
                    synced: task.remote_id.is_some(),
                    dirty,
                    commit_pushed,
                    messages: conversation
                        .messages
                        .into_iter()
                        .map(|message| DesktopMessage {
                            role: format!("{:?}", message.role),
                            agent: message.agent,
                            content: message.content,
                            created_at: message.created_at,
                        })
                        .collect(),
                    pending_handoffs,
                }
            })
            .collect(),
        profiles: config
            .profiles
            .into_iter()
            .map(|(name, profile)| DesktopProfile {
                name,
                command: profile.command,
                args: profile.args,
            })
            .collect(),
        users,
        notifications,
    })
}

#[tauri::command]
fn save_terminal(command: String, args: Vec<String>) -> Result<(), String> {
    if command.trim().is_empty() {
        return Err("terminal command is required".into());
    }
    let mut config = Config::load().map_err(to_string)?;
    config.terminal = akh_core::TerminalConfig { command, args };
    config.save().map_err(to_string)
}

#[tauri::command]
fn save_worktree_root(path: String) -> Result<(), String> {
    if path.trim().is_empty() {
        return Err("worktree directory is required".into());
    }
    let mut config = Config::load().map_err(to_string)?;
    config.worktree_root = PathBuf::from(path);
    config.save().map_err(to_string)
}

#[tauri::command]
fn save_profile(name: String, command: String, args: Vec<String>) -> Result<(), String> {
    akh_core::config::validate_name(&name, "profile").map_err(to_string)?;
    if command.trim().is_empty() {
        return Err("profile command is required".into());
    }
    let mut config = Config::load().map_err(to_string)?;
    config
        .profiles
        .insert(name, akh_core::AgentProfile { command, args });
    config.save().map_err(to_string)
}

#[tauri::command]
fn login_server(server: String, identity: String, password: String) -> Result<(), String> {
    let auth = akh_client::ServerClient::new(&server, None)
        .and_then(|client| client.login(&identity, &password))
        .map_err(to_string)?;
    save_server_auth(server, auth.token, auth.user_id)
}

#[tauri::command]
fn register_server(
    server: String,
    email: String,
    username: String,
    password: String,
) -> Result<(), String> {
    let auth = akh_client::ServerClient::new(&server, None)
        .and_then(|client| client.register(&email, &username, &password))
        .map_err(to_string)?;
    save_server_auth(server, auth.token, auth.user_id)
}

fn save_server_auth(server: String, token: String, user_id: uuid::Uuid) -> Result<(), String> {
    let mut config = Config::load().map_err(to_string)?;
    config.server = Some(server);
    config.token = Some(token);
    config.user_id = Some(user_id);
    config.save().map_err(to_string)
}

#[tauri::command]
fn logout_server() -> Result<(), String> {
    let mut config = Config::load().map_err(to_string)?;
    config.token = None;
    config.user_id = None;
    config.save().map_err(to_string)
}

#[tauri::command]
fn accept_handoff(id: String) -> Result<(), String> {
    let config = Config::load().map_err(to_string)?;
    let client = akh_client::ServerClient::new(
        config.server.as_deref().ok_or("server is not configured")?,
        Some(config.token.as_deref().ok_or("token is not configured")?),
    )
    .map_err(to_string)?;
    client
        .accept_handoff(id.parse().map_err(|_| "invalid handoff UUID")?)
        .map_err(to_string)?;
    Ok(())
}

#[tauri::command]
fn sync_now() -> Result<String, String> {
    let mut command = Command::new(akh_command_path());
    command.arg("sync");
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        command.creation_flags(0x08000000);
    }
    let output = command.output().map_err(to_string)?;
    if !output.status.success() {
        return Err(String::from_utf8_lossy(&output.stderr).trim().into());
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().into())
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
    let mut command = Command::new(&config.terminal.command);
    let akh = akh_command_path();
    command
        .args(&config.terminal.args)
        .arg(akh)
        .args(["task", &id.to_string(), &agent]);
    command.spawn().map_err(to_string)?;
    Ok(())
}

fn akh_command_path() -> PathBuf {
    std::env::current_exe()
        .ok()
        .and_then(|path| path.parent().map(|parent| parent.join("akh.exe")))
        .filter(|path| path.exists())
        .unwrap_or_else(|| PathBuf::from("akh"))
}

#[tauri::command]
fn handoff_task(id: u64, to: String, note: String) -> Result<(), String> {
    let config = Config::load().map_err(to_string)?;
    let task = config.task(id).map_err(to_string)?;
    let project = config.project(&task.project).map_err(to_string)?;
    let worktree = config
        .worktree_root
        .join(&task.project)
        .join(id.to_string());
    let repo = GitRepository::discover(if worktree.exists() {
        &worktree
    } else {
        &project.path
    })
    .map_err(to_string)?;
    if !repo.is_clean().map_err(to_string)? {
        return Err("task has uncommitted changes; commit them before handoff".into());
    }
    let commit = task
        .latest_commit
        .as_deref()
        .ok_or("task has no completed agent session commit")?;
    repo.fetch().map_err(to_string)?;
    if !repo
        .commit_is_pushed(commit, &task.branch)
        .map_err(to_string)?
    {
        return Err(format!(
            "commit {commit} is not available on origin/{}; push it before handoff",
            task.branch
        ));
    }
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
            import_backup,
            save_terminal,
            save_worktree_root,
            save_profile,
            login_server,
            register_server,
            logout_server,
            sync_now,
            accept_handoff
        ])
        .run(tauri::generate_context!())
        .expect("error while running Akh desktop");
}
