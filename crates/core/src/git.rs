use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result, bail};

#[derive(Debug, Clone)]
pub struct GitRepository {
    pub root: PathBuf,
    pub origin: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorktreeOutcome {
    Existing,
    CreatedFromLocalBranch,
    CreatedFromRemoteBranch,
    CreatedNewBranch,
}

impl GitRepository {
    pub fn discover(path: &Path) -> Result<Self> {
        let root = git_output(path, &["rev-parse", "--show-toplevel"])
            .with_context(|| format!("{} is not inside a Git repository", path.display()))?;
        let root = canonical(Path::new(&root))?;
        let origin = git_output(&root, &["remote", "get-url", "origin"]).ok();
        Ok(Self { root, origin })
    }

    pub fn fetch(&self) -> Result<()> {
        if self.origin.is_none() {
            return Ok(());
        }
        git_status(&self.root, &["fetch", "origin", "--prune"])
    }

    pub fn head(&self) -> Result<String> {
        git_output(&self.root, &["rev-parse", "HEAD"])
    }

    pub fn current_branch(&self) -> Result<String> {
        git_output(&self.root, &["symbolic-ref", "--short", "HEAD"])
            .context("repository has no active branch; choose a base branch explicitly")
    }

    pub fn is_clean(&self) -> Result<bool> {
        Ok(git_output(&self.root, &["status", "--porcelain"])?.is_empty())
    }

    pub fn commit_is_pushed(&self, commit: &str, branch: &str) -> Result<bool> {
        if self.origin.is_none() {
            return Ok(false);
        }
        let remote_branch = format!("refs/remotes/origin/{branch}");
        if !ref_exists(&self.root, &remote_branch) {
            return Ok(false);
        }
        Ok(Command::new("git")
            .args(["merge-base", "--is-ancestor", commit, &remote_branch])
            .current_dir(&self.root)
            .status()
            .context("failed to verify whether commit was pushed")?
            .success())
    }

    pub fn validate_branch(branch: &str) -> Result<()> {
        let status = Command::new("git")
            .args(["check-ref-format", "--branch", branch])
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .status()
            .context("failed to start Git; make sure it is installed and available on PATH")?;
        if !status.success() {
            bail!("'{branch}' is not a valid Git branch name");
        }
        Ok(())
    }

    pub fn ensure_worktree(
        &self,
        destination: &Path,
        branch: &str,
        base_branch: Option<&str>,
    ) -> Result<WorktreeOutcome> {
        Self::validate_branch(branch)?;
        if destination.exists() {
            let existing = Self::discover(destination)?;
            if existing.root != canonical(destination)? {
                bail!(
                    "{} exists but is not the expected worktree",
                    destination.display()
                );
            }
            let checked_out = git_output(destination, &["symbolic-ref", "--short", "HEAD"])?;
            if checked_out != branch {
                bail!(
                    "{} is on branch '{checked_out}', expected '{branch}'",
                    destination.display()
                );
            }
            return Ok(WorktreeOutcome::Existing);
        }

        if let Some(parent) = destination.parent() {
            std::fs::create_dir_all(parent)
                .with_context(|| format!("failed to create {}", parent.display()))?;
        }

        if ref_exists(&self.root, &format!("refs/heads/{branch}")) {
            git_status(
                &self.root,
                &["worktree", "add", path_arg(destination)?, branch],
            )?;
            return Ok(WorktreeOutcome::CreatedFromLocalBranch);
        }

        let remote = format!("origin/{branch}");
        if ref_exists(&self.root, &format!("refs/remotes/{remote}")) {
            git_status(
                &self.root,
                &[
                    "worktree",
                    "add",
                    "-b",
                    branch,
                    path_arg(destination)?,
                    &remote,
                ],
            )?;
            return Ok(WorktreeOutcome::CreatedFromRemoteBranch);
        }

        let current = self.current_branch().ok();
        let implicit_base = if base_branch.is_none() {
            Some(self.current_branch()?)
        } else {
            None
        };
        let base = base_branch.or(implicit_base.as_deref()).unwrap();
        let force_remote = base.starts_with("origin/");
        let base = base.strip_prefix("origin/").unwrap_or(base);
        Self::validate_branch(base)?;
        let remote_base = format!("origin/{base}");
        let start = if !force_remote
            && current.as_deref() == Some(base)
            && ref_exists(&self.root, &format!("refs/heads/{base}"))
        {
            base.to_owned()
        } else if ref_exists(&self.root, &format!("refs/remotes/{remote_base}")) {
            remote_base
        } else if ref_exists(&self.root, &format!("refs/heads/{base}")) {
            base.to_owned()
        } else {
            bail!("base branch '{base}' does not exist locally or on origin");
        };
        git_status(
            &self.root,
            &[
                "worktree",
                "add",
                "-b",
                branch,
                path_arg(destination)?,
                &start,
            ],
        )?;
        Ok(WorktreeOutcome::CreatedNewBranch)
    }
}

fn ref_exists(repo: &Path, reference: &str) -> bool {
    Command::new("git")
        .args(["show-ref", "--verify", "--quiet", reference])
        .current_dir(repo)
        .status()
        .is_ok_and(|status| status.success())
}

fn git_output(repo: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(repo)
        .output()
        .context("failed to start Git; make sure it is installed and available on PATH")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).trim().to_owned();
        bail!("git {} failed: {stderr}", args.join(" "));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
}

fn git_status(repo: &Path, args: &[&str]) -> Result<()> {
    let status = Command::new("git")
        .args(args)
        .current_dir(repo)
        .status()
        .context("failed to start Git; make sure it is installed and available on PATH")?;
    if !status.success() {
        bail!("git {} failed with {status}", args.join(" "));
    }
    Ok(())
}

fn canonical(path: &Path) -> Result<PathBuf> {
    path.canonicalize()
        .with_context(|| format!("failed to resolve {}", path.display()))
}

fn path_arg(path: &Path) -> Result<&str> {
    path.to_str().context("worktree path is not valid UTF-8")
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn discovers_repository_and_creates_task_worktree() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        fs::create_dir(&repo_path).unwrap();
        run(&repo_path, &["init", "-b", "main"]);
        run(&repo_path, &["config", "user.name", "Akh Test"]);
        run(
            &repo_path,
            &["config", "user.email", "akh-test@example.invalid"],
        );
        fs::write(repo_path.join("README.md"), "test\n").unwrap();
        run(&repo_path, &["add", "README.md"]);
        run(&repo_path, &["commit", "-m", "initial"]);

        let discovered = GitRepository::discover(&repo_path).unwrap();
        assert_eq!(discovered.root, repo_path.canonicalize().unwrap());
        assert!(discovered.origin.is_none());

        let worktree = temp.path().join("worktrees").join("1");
        let outcome = discovered
            .ensure_worktree(&worktree, "task/1", Some("main"))
            .unwrap();
        assert_eq!(outcome, WorktreeOutcome::CreatedNewBranch);
        assert_eq!(
            GitRepository::discover(&worktree).unwrap().root,
            worktree.canonicalize().unwrap()
        );
        assert_eq!(
            discovered
                .ensure_worktree(&worktree, "task/1", Some("main"))
                .unwrap(),
            WorktreeOutcome::Existing
        );
    }

    #[test]
    fn validates_branch_names() {
        assert!(GitRepository::validate_branch("task/183").is_ok());
        assert!(GitRepository::validate_branch("--upload-pack=bad").is_err());
        assert!(GitRepository::validate_branch("bad..branch").is_err());
    }

    #[test]
    fn creates_new_branch_from_selected_base_instead_of_current_head() {
        let temp = tempfile::tempdir().unwrap();
        let repo_path = temp.path().join("repo");
        fs::create_dir(&repo_path).unwrap();
        run(&repo_path, &["init", "-b", "main"]);
        run(&repo_path, &["config", "user.name", "Akh Test"]);
        run(
            &repo_path,
            &["config", "user.email", "akh-test@example.invalid"],
        );
        fs::write(repo_path.join("base.txt"), "main").unwrap();
        run(&repo_path, &["add", "."]);
        run(&repo_path, &["commit", "-m", "base"]);
        let main_commit = git_output(&repo_path, &["rev-parse", "HEAD"]).unwrap();
        run(&repo_path, &["checkout", "-b", "develop"]);
        fs::write(repo_path.join("base.txt"), "develop").unwrap();
        run(&repo_path, &["commit", "-am", "develop"]);

        let repo = GitRepository::discover(&repo_path).unwrap();
        let missing = temp.path().join("missing");
        assert_eq!(
            repo.ensure_worktree(&missing, "task/missing", None)
                .unwrap(),
            WorktreeOutcome::CreatedNewBranch
        );
        assert_eq!(
            git_output(&missing, &["rev-parse", "HEAD"]).unwrap(),
            repo.head().unwrap()
        );

        let worktree = temp.path().join("task");
        assert_eq!(
            repo.ensure_worktree(&worktree, "task/1", Some("main"))
                .unwrap(),
            WorktreeOutcome::CreatedNewBranch
        );
        assert_eq!(
            git_output(&worktree, &["rev-parse", "HEAD"]).unwrap(),
            main_commit
        );
        assert_eq!(
            git_output(&worktree, &["symbolic-ref", "--short", "HEAD"]).unwrap(),
            "task/1"
        );
    }

    fn run(repo: &Path, args: &[&str]) {
        let status = Command::new("git")
            .args(args)
            .current_dir(repo)
            .status()
            .unwrap();
        assert!(status.success(), "git {} failed", args.join(" "));
    }
}
