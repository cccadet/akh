use std::path::PathBuf;

use akh_client::{CreateMessage, CreateProject, CreateTask, ServerClient, UpdateTask};
use akh_core::config::validate_name;
use akh_core::{Config, Conversation, GitRepository, ProjectLink, Role, TaskLink};
use anyhow::{Context, Result, bail};
use clap::{Args, Parser, Subcommand};

#[derive(Parser)]
#[command(
    name = "akh",
    version,
    about = "Shared task continuity for coding agents"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Subcommand)]
enum Command {
    Register(AuthArgs),
    Login(LoginArgs),
    Sync,
    Handoff(HandoffArgs),
    Handoffs {
        task_id: u64,
    },
    AcceptHandoff {
        id: String,
    },
    Users,
    Notifications,
    Project(ProjectArgs),
    #[command(name = "task-link")]
    TaskLink(TaskLinkArgs),
    Task(TaskArgs),
    Backup(BackupArgs),
    Profile(ProfileArgs),
    Terminal(TerminalArgs),
    Worktree(WorktreeArgs),
    ConfigPath,
}

#[derive(Args)]
struct HandoffArgs {
    task_id: u64,
    #[arg(long)]
    to: String,
    #[arg(long)]
    note: String,
}

#[derive(Args)]
struct AuthArgs {
    #[arg(long)]
    server: String,
    #[arg(long)]
    email: String,
    #[arg(long)]
    username: String,
}

#[derive(Args)]
struct LoginArgs {
    #[arg(long)]
    server: String,
    identity: String,
}

#[derive(Args)]
struct ProjectArgs {
    #[command(subcommand)]
    command: ProjectCommand,
}

#[derive(Subcommand)]
enum ProjectCommand {
    Detect {
        path: Option<PathBuf>,
    },
    Link {
        path: Option<PathBuf>,
        #[arg(long)]
        name: Option<String>,
    },
    List,
}

#[derive(Args)]
struct TaskLinkArgs {
    id: u64,
    #[arg(long)]
    project: String,
    #[arg(long)]
    branch: String,
}

#[derive(Args)]
struct TaskArgs {
    target: String,
    value: Option<String>,
    #[arg(long)]
    project: Option<String>,
    #[arg(long)]
    branch: Option<String>,
    #[arg(long)]
    prepare_only: bool,
}

#[derive(Args)]
struct BackupArgs {
    #[command(subcommand)]
    command: BackupCommand,
}

#[derive(Subcommand)]
enum BackupCommand {
    Export {
        path: PathBuf,
        #[arg(long)]
        database: Option<PathBuf>,
    },
    Import {
        path: PathBuf,
        #[arg(long)]
        database: Option<PathBuf>,
    },
}

#[derive(Args)]
struct ProfileArgs {
    #[command(subcommand)]
    command: ProfileCommand,
}

#[derive(Subcommand)]
enum ProfileCommand {
    List,
    Set {
        name: String,
        command: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
    Remove {
        name: String,
    },
}

#[derive(Args)]
struct TerminalArgs {
    #[command(subcommand)]
    command: TerminalCommand,
}

#[derive(Args)]
struct WorktreeArgs {
    #[command(subcommand)]
    command: WorktreeCommand,
}

#[derive(Subcommand)]
enum WorktreeCommand {
    Show,
    Set { path: PathBuf },
}

#[derive(Subcommand)]
enum TerminalCommand {
    Show,
    Set {
        command: String,
        #[arg(trailing_var_arg = true)]
        args: Vec<String>,
    },
}

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Register(args) => register(args),
        Command::Login(args) => login(args),
        Command::Sync => sync(),
        Command::Handoff(args) => handoff(args),
        Command::Handoffs { task_id } => list_handoffs(task_id),
        Command::AcceptHandoff { id } => accept_handoff(&id),
        Command::Users => list_users(),
        Command::Notifications => list_notifications(),
        Command::Project(args) => project(args.command),
        Command::TaskLink(args) => task_link(args),
        Command::Task(args) => task(args),
        Command::Backup(args) => backup(args.command),
        Command::Profile(args) => profile(args.command),
        Command::Terminal(args) => terminal(args.command),
        Command::Worktree(args) => worktree(args.command),
        Command::ConfigPath => {
            println!("{}", Config::path()?.display());
            Ok(())
        }
    }
}

fn authenticated_client() -> Result<ServerClient> {
    let config = Config::load()?;
    ServerClient::new(
        config
            .server
            .as_deref()
            .context("server is not configured")?,
        Some(config.token.as_deref().context("token is not configured")?),
    )
}

fn list_users() -> Result<()> {
    for user in authenticated_client()?.users()? {
        println!("{}\t{}\t{}", user.id, user.username, user.email);
    }
    Ok(())
}

fn list_notifications() -> Result<()> {
    for notification in authenticated_client()?.notifications()? {
        println!(
            "{}\t{}\t{}",
            notification.id, notification.kind, notification.content
        );
    }
    Ok(())
}

fn list_handoffs(task_id: u64) -> Result<()> {
    let config = Config::load()?;
    let remote_id = config
        .task(task_id)?
        .remote_id
        .context("task is not synchronized")?;
    for handoff in authenticated_client()?.handoffs(remote_id)? {
        println!(
            "{}\t{} -> {}\t{}",
            handoff.id, handoff.from_user_id, handoff.to_user_id, handoff.note
        );
    }
    Ok(())
}

fn accept_handoff(id: &str) -> Result<()> {
    let id = id.parse().context("invalid handoff UUID")?;
    let handoff = authenticated_client()?.accept_handoff(id)?;
    println!(
        "accepted handoff {} for task {}",
        handoff.id, handoff.task_id
    );
    Ok(())
}

fn worktree(command: WorktreeCommand) -> Result<()> {
    let mut config = Config::load()?;
    match command {
        WorktreeCommand::Show => println!("{}", config.worktree_root.display()),
        WorktreeCommand::Set { path } => {
            config.worktree_root = path;
            config.save()?;
            println!("saved worktree root");
        }
    }
    Ok(())
}

fn profile(command: ProfileCommand) -> Result<()> {
    let mut config = Config::load()?;
    match command {
        ProfileCommand::List => {
            for (name, profile) in config.profiles {
                println!("{name}\t{} {}", profile.command, profile.args.join(" "));
            }
        }
        ProfileCommand::Set {
            name,
            command,
            args,
        } => {
            validate_name(&name, "profile")?;
            config
                .profiles
                .insert(name.clone(), akh_core::AgentProfile { command, args });
            config.save()?;
            println!("saved profile '{name}'");
        }
        ProfileCommand::Remove { name } => {
            config
                .profiles
                .remove(&name)
                .with_context(|| format!("profile '{name}' does not exist"))?;
            config.save()?;
            println!("removed profile '{name}'");
        }
    }
    Ok(())
}

fn terminal(command: TerminalCommand) -> Result<()> {
    let mut config = Config::load()?;
    match command {
        TerminalCommand::Show => println!(
            "{} {}",
            config.terminal.command,
            config.terminal.args.join(" ")
        ),
        TerminalCommand::Set { command, args } => {
            config.terminal = akh_core::TerminalConfig { command, args };
            config.save()?;
            println!("saved terminal configuration");
        }
    }
    Ok(())
}

fn backup(command: BackupCommand) -> Result<()> {
    match command {
        BackupCommand::Export { path, database } => {
            let snapshot = if let Some(database) = database.as_deref() {
                let snapshot = database.with_extension("backup-snapshot.db");
                tokio::runtime::Runtime::new()?
                    .block_on(akh_local_server::sanitized_snapshot(database, &snapshot))?;
                Some(snapshot)
            } else {
                None
            };
            let export = akh_core::export_backup(&path, snapshot.as_deref());
            if let Some(snapshot) = snapshot {
                std::fs::remove_file(snapshot)?;
            }
            let result = export?;
            println!(
                "exported {} projects, {} tasks and {} conversations to {}",
                result.projects,
                result.tasks,
                result.conversations,
                path.display()
            );
        }
        BackupCommand::Import { path, database } => {
            let result = akh_core::import_backup(&path, database.as_deref())?;
            println!(
                "imported {} projects, {} tasks and {} conversations",
                result.projects, result.tasks, result.conversations
            );
            if result.restart_required {
                println!("restart Akh to activate the imported SQLite database");
            }
            println!("relink imported projects before launching their tasks");
        }
    }
    Ok(())
}

fn project(command: ProjectCommand) -> Result<()> {
    match command {
        ProjectCommand::Detect { path } => {
            let repo = GitRepository::discover(&path_or_current(path)?)?;
            println!("root: {}", repo.root.display());
            println!("origin: {}", repo.origin.as_deref().unwrap_or("<none>"));
            Ok(())
        }
        ProjectCommand::Link { path, name } => {
            let repo = GitRepository::discover(&path_or_current(path)?)?;
            let inferred = repo
                .root
                .file_name()
                .and_then(|value| value.to_str())
                .context("could not infer project name; provide --name")?;
            let name = name.unwrap_or_else(|| inferred.to_owned());
            validate_name(&name, "project")?;

            let mut config = Config::load()?;
            config.projects.insert(
                name.clone(),
                ProjectLink {
                    path: repo.root.clone(),
                    repository_url: repo.origin.clone(),
                    remote_id: None,
                },
            );
            config.save()?;
            println!("linked project '{name}' to {}", repo.root.display());
            Ok(())
        }
        ProjectCommand::List => {
            let config = Config::load()?;
            if config.projects.is_empty() {
                println!("no linked projects");
            }
            for (name, project) in config.projects {
                println!(
                    "{name}\t{}\t{}",
                    project.path.display(),
                    project.repository_url.as_deref().unwrap_or("<none>")
                );
            }
            Ok(())
        }
    }
}

fn task_link(args: TaskLinkArgs) -> Result<()> {
    validate_name(&args.project, "project")?;
    GitRepository::validate_branch(&args.branch)?;
    let mut config = Config::load()?;
    config.project(&args.project)?;
    config.tasks.insert(
        args.id,
        TaskLink {
            project: args.project,
            branch: args.branch,
            title: format!("Task #{}", args.id),
            latest_commit: None,
            remote_id: None,
        },
    );
    config.save()?;
    println!("linked task {}", args.id);
    Ok(())
}

fn task(args: TaskArgs) -> Result<()> {
    match args.target.as_str() {
        "create" => return task_create(args),
        "list" => return task_list(),
        "show" => return task_show(args),
        _ => {}
    }

    let id = args
        .target
        .parse::<u64>()
        .with_context(|| format!("'{}' is not a task id", args.target))?;
    let agent = args
        .value
        .context("missing agent profile (for example: codex)")?;
    sync_if_configured("before agent launch");
    validate_name(&agent, "agent profile")?;
    let mut config = Config::load()?;
    let task = config.task(id)?.clone();
    let project = config.project(&task.project)?.clone();
    let profile = config.profile(&agent)?.clone();
    let repo = GitRepository::discover(&project.path)?;
    if let Some(expected) = &project.repository_url
        && repo.origin.as_ref() != Some(expected)
    {
        bail!(
            "project '{}' now points to a different origin (expected '{}', found '{}')",
            task.project,
            expected,
            repo.origin.as_deref().unwrap_or("<none>")
        );
    }
    repo.fetch()?;

    let worktree = config
        .worktree_root
        .join(&task.project)
        .join(id.to_string());
    let outcome = repo.ensure_worktree(&worktree, &task.branch)?;
    println!("task {id}: {outcome:?}");
    println!("worktree: {}", worktree.display());

    if args.prepare_only {
        return Ok(());
    }
    let worktree_repo = GitRepository::discover(&worktree)?;
    let commit = worktree_repo.head()?;
    let mut conversation = Conversation::load(id)?;
    let context = conversation.context(&task, &commit);
    let remote_session = match (
        task.remote_id,
        config.server.as_deref(),
        config.token.as_deref(),
    ) {
        (Some(remote_id), Some(server), Some(token)) => {
            match ServerClient::new(server, Some(token))
                .and_then(|client| client.start_session(remote_id, &agent))
            {
                Ok(session) => Some((server.to_owned(), token.to_owned(), session.id)),
                Err(error) => {
                    eprintln!("warning: could not register remote session: {error}");
                    None
                }
            }
        }
        _ => None,
    };
    let session = akh_core::launch::launch(&profile, &worktree, &context)?;
    conversation.append(Role::User, &agent, session.input);
    conversation.append(Role::Assistant, &agent, session.output);
    conversation.save()?;

    let latest_commit = worktree_repo.head()?;
    if !worktree_repo.is_clean()? {
        eprintln!(
            "warning: task {id} has uncommitted changes; they remain local and will not be shared"
        );
    }
    if let Some(stored_task) = config.tasks.get_mut(&id) {
        stored_task.latest_commit = Some(latest_commit);
    }
    config.save()?;
    if let Some((server, token, session_id)) = remote_session
        && let Err(error) = ServerClient::new(&server, Some(&token))
            .and_then(|client| client.end_session(session_id))
    {
        eprintln!("warning: could not close remote session: {error}");
    }
    if !session.success {
        bail!("agent '{agent}' exited unsuccessfully");
    }
    sync_if_configured("after agent exit");
    Ok(())
}

fn task_create(args: TaskArgs) -> Result<()> {
    let title = args.value.context("missing task title")?;
    let mut config = Config::load()?;
    let project = match args.project {
        Some(project) => project,
        None => detect_linked_project(&config)?,
    };
    validate_name(&project, "project")?;
    config.project(&project)?;
    let id = config.tasks.keys().next_back().copied().unwrap_or(0) + 1;
    let branch = args.branch.unwrap_or_else(|| format!("task/{id}"));
    GitRepository::validate_branch(&branch)?;
    config.tasks.insert(
        id,
        TaskLink {
            project,
            branch,
            title,
            latest_commit: None,
            remote_id: None,
        },
    );
    config.save()?;
    println!("created task {id}");
    Ok(())
}

fn detect_linked_project(config: &Config) -> Result<String> {
    let current = GitRepository::discover(&std::env::current_dir()?)?;
    config
        .projects
        .iter()
        .find(|(_, project)| {
            project.path == current.root
                || (project.repository_url.is_some() && project.repository_url == current.origin)
        })
        .map(|(name, _)| name.clone())
        .context("current repository is not linked; run `akh project link` or provide --project")
}

fn register(args: AuthArgs) -> Result<()> {
    let password = rpassword::prompt_password("Password: ")?;
    let client = ServerClient::new(&args.server, None)?;
    let auth = client.register(&args.email, &args.username, &password)?;
    save_auth(args.server, auth.token, auth.user_id)?;
    println!("registered and logged in as {}", auth.user_id);
    Ok(())
}

fn login(args: LoginArgs) -> Result<()> {
    let password = rpassword::prompt_password("Password: ")?;
    let client = ServerClient::new(&args.server, None)?;
    let auth = client.login(&args.identity, &password)?;
    save_auth(args.server, auth.token, auth.user_id)?;
    println!("logged in as {}", auth.user_id);
    Ok(())
}

fn save_auth(server: String, token: String, user_id: uuid::Uuid) -> Result<()> {
    let mut config = Config::load()?;
    config.server = Some(server);
    config.token = Some(token);
    config.user_id = Some(user_id);
    config.save()
}

fn handoff(args: HandoffArgs) -> Result<()> {
    let config = Config::load()?;
    let task = config.task(args.task_id)?;
    let project = config.project(&task.project)?;
    let worktree = config
        .worktree_root
        .join(&task.project)
        .join(args.task_id.to_string());
    let repo = GitRepository::discover(if worktree.exists() {
        &worktree
    } else {
        &project.path
    })?;
    if !repo.is_clean()? {
        bail!("task has uncommitted changes; commit them before handoff");
    }
    let commit = task
        .latest_commit
        .as_deref()
        .context("task has no completed agent session commit")?;
    repo.fetch()?;
    if !repo.commit_is_pushed(commit, &task.branch)? {
        bail!(
            "commit {commit} is not available on origin/{}; push it before handoff",
            task.branch
        );
    }
    let remote_id = task
        .remote_id
        .context("task is not synchronized; run `akh sync`")?;
    let server = config
        .server
        .as_deref()
        .context("server is not configured")?;
    let token = config.token.as_deref().context("token is not configured")?;
    let recipient = args.to.parse().context("--to must be a user UUID")?;
    let client = ServerClient::new(server, Some(token))?;
    let handoff = client.create_handoff(remote_id, recipient, &args.note)?;
    println!("handoff {} created for task {}", handoff.id, args.task_id);
    Ok(())
}

fn sync() -> Result<()> {
    let mut config = Config::load()?;
    let server = config
        .server
        .as_deref()
        .context("server is not configured; run `akh login`")?;
    let token = config
        .token
        .as_deref()
        .context("token is not configured; run `akh login`")?;
    let client = ServerClient::new(server, Some(token))?;
    let mut remote_projects = client.projects()?;

    let project_names: Vec<String> = config.projects.keys().cloned().collect();
    for name in project_names {
        let project = config.projects.get_mut(&name).unwrap();
        let repository_url = project
            .repository_url
            .as_deref()
            .with_context(|| format!("project '{name}' has no origin"))?;
        let remote = match remote_projects
            .iter()
            .find(|item| item.repository_url == repository_url)
        {
            Some(remote) => remote.clone(),
            None => {
                let created = client.create_project(&CreateProject {
                    name: &name,
                    repository_url,
                    default_branch: "main",
                })?;
                remote_projects.push(created.clone());
                created
            }
        };
        project.remote_id = Some(remote.id);
    }

    let linked_projects: Vec<_> = config
        .projects
        .iter()
        .filter_map(|(name, project)| project.remote_id.map(|id| (name.clone(), id)))
        .collect();
    for (project_name, project_id) in linked_projects {
        for remote in client.tasks(Some(project_id))? {
            if config
                .tasks
                .values()
                .any(|task| task.remote_id == Some(remote.id))
            {
                continue;
            }
            let preferred = u64::try_from(remote.id).unwrap_or(0);
            let local_id = if preferred > 0 && !config.tasks.contains_key(&preferred) {
                preferred
            } else {
                config.tasks.keys().next_back().copied().unwrap_or(0) + 1
            };
            config.tasks.insert(
                local_id,
                TaskLink {
                    project: project_name.clone(),
                    branch: remote.branch,
                    title: remote.title,
                    latest_commit: remote.latest_commit,
                    remote_id: Some(remote.id),
                },
            );
        }
    }

    let task_ids: Vec<u64> = config.tasks.keys().copied().collect();
    for local_id in task_ids {
        let mut task = config.tasks[&local_id].clone();
        let project_id = config
            .project(&task.project)?
            .remote_id
            .context("project was not synchronized")?;
        let remote_id = match task.remote_id {
            Some(id) => id,
            None => {
                client
                    .create_task(&CreateTask {
                        project_id,
                        title: &task.title,
                        description: "",
                        branch: &task.branch,
                    })?
                    .id
            }
        };
        task.remote_id = Some(remote_id);
        if let Some(commit) = task.latest_commit.as_deref() {
            let project = config.project(&task.project)?;
            let repo = GitRepository::discover(&project.path)?;
            repo.fetch()?;
            if repo.commit_is_pushed(commit, &task.branch)? {
                client.update_task(
                    remote_id,
                    &UpdateTask {
                        latest_commit: Some(commit),
                        status: Some("in_progress"),
                    },
                )?;
            } else {
                eprintln!(
                    "warning: task {local_id} commit {commit} is not on origin/{}; code state was not shared",
                    task.branch
                );
            }
        }

        let remote_messages = client.messages(remote_id)?;
        let mut conversation = Conversation::load(local_id)?;
        for message in &mut conversation.messages {
            if message.remote_id.is_none() {
                let role = match message.role {
                    Role::User => "user",
                    Role::Assistant => "assistant",
                };
                let remote = client.create_message(
                    remote_id,
                    &CreateMessage {
                        agent: &message.agent,
                        role,
                        content: &message.content,
                    },
                )?;
                message.remote_id = Some(remote.id);
            }
        }
        for remote in remote_messages {
            if conversation
                .messages
                .iter()
                .any(|message| message.remote_id == Some(remote.id))
            {
                continue;
            }
            conversation.messages.push(akh_core::Message {
                remote_id: Some(remote.id),
                role: if remote.role == "user" {
                    Role::User
                } else {
                    Role::Assistant
                },
                agent: remote.agent,
                content: remote.content,
                created_at: remote.created_at.timestamp().max(0) as u64,
            });
        }
        conversation
            .messages
            .sort_by_key(|message| message.created_at);
        conversation.save()?;
        config.tasks.insert(local_id, task);
    }
    config.save()?;
    println!(
        "synchronized {} projects and {} tasks",
        config.projects.len(),
        config.tasks.len()
    );
    Ok(())
}

fn sync_if_configured(stage: &str) {
    let configured = Config::load()
        .map(|config| config.server.is_some() && config.token.is_some())
        .unwrap_or(false);
    if configured && let Err(error) = sync() {
        eprintln!("warning: synchronization {stage} failed: {error}");
    }
}

fn task_list() -> Result<()> {
    let config = Config::load()?;
    if config.tasks.is_empty() {
        println!("no tasks");
    }
    for (id, task) in config.tasks {
        println!(
            "{id}\t{}\t{}\t{}\t{}",
            task.project,
            task.branch,
            task.title,
            task.latest_commit.as_deref().unwrap_or("<not started>")
        );
    }
    Ok(())
}

fn task_show(args: TaskArgs) -> Result<()> {
    let id = args
        .value
        .context("missing task id")?
        .parse::<u64>()
        .context("invalid task id")?;
    let config = Config::load()?;
    let task = config.task(id)?;
    println!("Task #{id}: {}", task.title);
    println!("Project: {}", task.project);
    println!("Branch: {}", task.branch);
    println!(
        "Latest commit: {}",
        task.latest_commit.as_deref().unwrap_or("<not started>")
    );
    let conversation = Conversation::load(id)?;
    println!("Messages: {}", conversation.messages.len());
    for message in conversation.messages {
        println!(
            "\n{:?} / {}:\n{}",
            message.role, message.agent, message.content
        );
    }
    Ok(())
}

fn path_or_current(path: Option<PathBuf>) -> Result<PathBuf> {
    path.map(Ok)
        .unwrap_or_else(|| std::env::current_dir().context("failed to read current directory"))
}
