use std::path::PathBuf;

use akh_core::config::validate_name;
use akh_core::{Config, GitRepository, ProjectLink, TaskLink};
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
    id: u64,
    agent: String,
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
        },
    );
    config.save()?;
    println!("linked task {}", args.id);
    Ok(())
}

fn task(args: TaskArgs) -> Result<()> {
    validate_name(&args.agent, "agent profile")?;
    let config = Config::load()?;
    let task = config.task(args.id)?;
    let project = config.project(&task.project)?;
    let profile = config.profile(&args.agent)?;
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
        .join(args.id.to_string());
    let outcome = repo.ensure_worktree(&worktree, &task.branch)?;
    println!("task {}: {:?}", args.id, outcome);
    println!("worktree: {}", worktree.display());

    if args.prepare_only {
        return Ok(());
    }
    let status = akh_core::launch::launch(profile, &worktree)?;
    if !status.success() {
        bail!("agent '{}' exited with {status}", args.agent);
    }
    Ok(())
}

fn path_or_current(path: Option<PathBuf>) -> Result<PathBuf> {
    path.map(Ok)
        .unwrap_or_else(|| std::env::current_dir().context("failed to read current directory"))
}
