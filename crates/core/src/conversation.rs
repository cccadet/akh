use std::fs;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::{Config, TaskLink};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Conversation {
    pub task_id: u64,
    #[serde(default)]
    pub messages: Vec<Message>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    #[serde(default)]
    pub remote_id: Option<uuid::Uuid>,
    pub role: Role,
    pub agent: String,
    pub content: String,
    pub created_at: u64,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Role {
    User,
    Assistant,
}

impl Conversation {
    pub fn load(task_id: u64) -> Result<Self> {
        let path = path(task_id)?;
        if !path.exists() {
            return Ok(Self {
                task_id,
                messages: Vec::new(),
            });
        }
        let contents = fs::read_to_string(&path)
            .with_context(|| format!("failed to read {}", path.display()))?;
        toml::from_str(&contents).with_context(|| format!("failed to parse {}", path.display()))
    }

    pub fn save(&self) -> Result<()> {
        let path = path(self.task_id)?;
        let parent = path.parent().context("conversation path has no parent")?;
        fs::create_dir_all(parent)
            .with_context(|| format!("failed to create {}", parent.display()))?;
        let contents = toml::to_string_pretty(self).context("failed to serialize conversation")?;
        fs::write(&path, contents).with_context(|| format!("failed to write {}", path.display()))
    }

    pub fn append(&mut self, role: Role, agent: &str, content: String) {
        let content = content.trim().to_owned();
        if content.is_empty() {
            return;
        }
        self.messages.push(Message {
            remote_id: None,
            role,
            agent: agent.to_owned(),
            content,
            created_at: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs(),
        });
    }

    pub fn context(&self, task: &TaskLink, commit: &str) -> String {
        let mut result = format!(
            "You are continuing Akh Task #{}\n\nPROJECT\n{}\n\nTASK\n{}\n\nBRANCH\n{}\n\nCURRENT COMMIT\n{}\n",
            self.task_id, task.project, task.title, task.branch, commit
        );
        if !self.messages.is_empty() {
            result.push_str("\nSHARED HISTORY\n\n");
            for message in &self.messages {
                let role = match message.role {
                    Role::User => "User",
                    Role::Assistant => "Assistant",
                };
                result.push_str(&format!(
                    "{} / {}:\n{}\n\n",
                    message.agent, role, message.content
                ));
            }
        }
        result.push_str("\nCURRENT STATE\n\nInspect the current repository and continue the task.");
        result
    }
}

fn path(task_id: u64) -> Result<PathBuf> {
    Ok(Config::data_dir()?
        .join("conversations")
        .join(format!("{task_id}.toml")))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builds_context_from_shared_history() {
        let mut conversation = Conversation {
            task_id: 183,
            messages: Vec::new(),
        };
        conversation.append(Role::User, "claude", "Implement wallet".into());
        let task = TaskLink {
            project: "omni-sql".into(),
            branch: "task/183".into(),
            title: "Oracle Wallet".into(),
            latest_commit: None,
            remote_id: None,
        };
        let context = conversation.context(&task, "abc123");
        assert!(context.contains("Akh Task #183"));
        assert!(context.contains("Implement wallet"));
        assert!(context.contains("abc123"));
    }
}
