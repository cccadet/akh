use std::path::PathBuf;

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
    Project(ProjectArgs),
    #[command(name = "task-link")]
    TaskLink(TaskLinkArgs),
    Task(TaskArgs),
    ConfigPath,
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
    let session = akh_core::launch::launch(&profile, &worktree, &context)?;
    conversation.append(Role::User, &agent, session.input);
    conversation.append(Role::Assistant, &agent, session.output);
    conversation.save()?;

    let latest_commit = worktree_repo.head()?;
    if let Some(stored_task) = config.tasks.get_mut(&id) {
        stored_task.latest_commit = Some(latest_commit);
    }
    config.save()?;
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
        },
    );
    config.save()?;
    println!("created task {id}");
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
