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
    Project(ProjectArgs),
    #[command(name = "task-link")]
    TaskLink(TaskLinkArgs),
    Task(TaskArgs),
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

fn main() -> Result<()> {
    match Cli::parse().command {
        Command::Register(args) => register(args),
        Command::Login(args) => login(args),
        Command::Sync => sync(),
        Command::Handoff(args) => handoff(args),
        Command::Project(args) => project(args.command),
        Command::TaskLink(args) => task_link(args),
        Command::Task(args) => task(args),
        Command::ConfigPath => {
            println!("{}", Config::path()?.display());
            Ok(())
        }
    }
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
    Ok(())
}

fn task_create(args: TaskArgs) -> Result<()> {
    let title = args.value.context("missing task title")?;
    let project = args.project.context("--project is required")?;
    validate_name(&project, "project")?;
    let mut config = Config::load()?;
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

fn register(args: AuthArgs) -> Result<()> {
    let password = rpassword::prompt_password("Password: ")?;
    let client = ServerClient::new(&args.server, None)?;
    let auth = client.register(&args.email, &args.username, &password)?;
    save_auth(args.server, auth.token)?;
    println!("registered and logged in as {}", auth.user_id);
    Ok(())
}

fn login(args: LoginArgs) -> Result<()> {
    let password = rpassword::prompt_password("Password: ")?;
    let client = ServerClient::new(&args.server, None)?;
    let auth = client.login(&args.identity, &password)?;
    save_auth(args.server, auth.token)?;
    println!("logged in as {}", auth.user_id);
    Ok(())
}

fn save_auth(server: String, token: String) -> Result<()> {
    let mut config = Config::load()?;
    config.server = Some(server);
    config.token = Some(token);
    config.save()
}

fn handoff(args: HandoffArgs) -> Result<()> {
    let config = Config::load()?;
    let task = config.task(args.task_id)?;
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
            client.update_task(
                remote_id,
                &UpdateTask {
                    latest_commit: Some(commit),
                    status: Some("in_progress"),
                },
            )?;
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
