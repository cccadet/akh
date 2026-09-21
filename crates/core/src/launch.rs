use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};

use anyhow::{Context, Result};

use crate::AgentProfile;

pub fn launch(profile: &AgentProfile, worktree: &Path) -> Result<ExitStatus> {
    Command::new(&profile.command)
        .args(&profile.args)
        .current_dir(worktree)
        .stdin(Stdio::inherit())
        .stdout(Stdio::inherit())
        .stderr(Stdio::inherit())
        .status()
        .with_context(|| {
            format!(
                "failed to start '{}' in {}; check the agent profile in ~/.akh/config.toml",
                profile.command,
                worktree.display()
            )
        })
}
