use anyhow::{Context, Result, bail};
use chrono::{DateTime, Utc};
use reqwest::blocking::{Client, RequestBuilder};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use std::time::Duration;
use uuid::Uuid;

pub struct ServerClient {
    base_url: String,
    token: Option<String>,
    http: Client,
}

impl ServerClient {
    pub fn new(base_url: &str, token: Option<&str>) -> Result<Self> {
        Ok(Self {
            base_url: base_url.trim_end_matches('/').to_owned(),
            token: token.map(str::to_owned),
            http: Client::builder().timeout(Duration::from_secs(10)).build()?,
        })
    }

    pub fn register(&self, email: &str, username: &str, password: &str) -> Result<AuthResponse> {
        self.send(
            self.http
                .post(self.url("/auth/register"))
                .json(&RegisterRequest {
                    email,
                    username,
                    password,
                }),
        )
    }

    pub fn login(&self, identity: &str, password: &str) -> Result<AuthResponse> {
        self.send(
            self.http
                .post(self.url("/auth/login"))
                .json(&LoginRequest { identity, password }),
        )
    }

    pub fn projects(&self) -> Result<Vec<Project>> {
        self.send(self.auth(self.http.get(self.url("/projects"))))
    }
    pub fn create_project(&self, input: &CreateProject) -> Result<Project> {
        self.send(self.auth(self.http.post(self.url("/projects"))).json(input))
    }
    pub fn create_task(&self, input: &CreateTask) -> Result<Task> {
        self.send(self.auth(self.http.post(self.url("/tasks"))).json(input))
    }
    pub fn tasks(&self, project_id: Option<Uuid>) -> Result<Vec<Task>> {
        let request = self.auth(self.http.get(self.url("/tasks")));
        let request = match project_id {
            Some(project_id) => request.query(&[("project_id", project_id)]),
            None => request,
        };
        self.send(request)
    }
    pub fn update_task(&self, id: i64, input: &UpdateTask) -> Result<Task> {
        self.send(
            self.auth(self.http.patch(self.url(&format!("/tasks/{id}"))))
                .json(input),
        )
    }
    pub fn messages(&self, task_id: i64) -> Result<Vec<RemoteMessage>> {
        self.send(
            self.auth(
                self.http
                    .get(self.url(&format!("/tasks/{task_id}/messages"))),
            ),
        )
    }
    pub fn create_message(&self, task_id: i64, input: &CreateMessage) -> Result<RemoteMessage> {
        self.send(
            self.auth(
                self.http
                    .post(self.url(&format!("/tasks/{task_id}/messages"))),
            )
            .json(input),
        )
    }

    pub fn create_handoff(&self, task_id: i64, to_user_id: Uuid, note: &str) -> Result<Handoff> {
        self.send(
            self.auth(
                self.http
                    .post(self.url(&format!("/tasks/{task_id}/handoffs"))),
            )
            .json(&CreateHandoff { to_user_id, note }),
        )
    }
    pub fn users(&self) -> Result<Vec<User>> {
        self.send(self.auth(self.http.get(self.url("/users"))))
    }

    pub fn handoffs(&self, task_id: i64) -> Result<Vec<Handoff>> {
        self.send(
            self.auth(
                self.http
                    .get(self.url(&format!("/tasks/{task_id}/handoffs"))),
            ),
        )
    }

    pub fn accept_handoff(&self, id: Uuid) -> Result<Handoff> {
        self.send(self.auth(self.http.post(self.url(&format!("/handoffs/{id}/accept")))))
    }

    pub fn notifications(&self) -> Result<Vec<Notification>> {
        self.send(self.auth(self.http.get(self.url("/notifications"))))
    }

    pub fn start_session(&self, task_id: i64, agent: &str) -> Result<AgentSession> {
        self.send(
            self.auth(
                self.http
                    .post(self.url(&format!("/tasks/{task_id}/sessions"))),
            )
            .json(&StartSession { agent }),
        )
    }

    pub fn end_session(&self, id: Uuid) -> Result<AgentSession> {
        self.send(
            self.auth(self.http.patch(self.url(&format!("/sessions/{id}"))))
                .json(&serde_json::json!({})),
        )
    }

    fn auth(&self, request: RequestBuilder) -> RequestBuilder {
        match &self.token {
            Some(token) => request.bearer_auth(token),
            None => request,
        }
    }

    fn send<T: DeserializeOwned>(&self, request: RequestBuilder) -> Result<T> {
        let response = request.send().context("failed to contact Akh server")?;
        let status = response.status();
        if !status.is_success() {
            let body = response.text().unwrap_or_default();
            bail!("Akh server returned {status}: {body}");
        }
        response.json().context("invalid response from Akh server")
    }

    fn url(&self, path: &str) -> String {
        format!("{}{}", self.base_url, path)
    }
}

#[derive(Debug, Deserialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: Uuid,
}
#[derive(Serialize)]
struct RegisterRequest<'a> {
    email: &'a str,
    username: &'a str,
    password: &'a str,
}
#[derive(Serialize)]
struct LoginRequest<'a> {
    identity: &'a str,
    password: &'a str,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Project {
    pub id: Uuid,
    pub name: String,
    pub repository_url: String,
    pub default_branch: String,
}
#[derive(Serialize)]
pub struct CreateProject<'a> {
    pub name: &'a str,
    pub repository_url: &'a str,
    pub default_branch: &'a str,
}

#[derive(Debug, Clone, Deserialize)]
pub struct Task {
    pub id: i64,
    pub project_id: Uuid,
    pub title: String,
    pub description: String,
    pub branch: String,
    #[serde(default)]
    pub base_branch: Option<String>,
    pub latest_commit: Option<String>,
    pub status: String,
}
#[derive(Serialize)]
pub struct CreateTask<'a> {
    pub project_id: Uuid,
    pub title: &'a str,
    pub description: &'a str,
    pub branch: &'a str,
    pub base_branch: Option<&'a str>,
}
#[derive(Default, Serialize)]
pub struct UpdateTask<'a> {
    pub latest_commit: Option<&'a str>,
    pub status: Option<&'a str>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RemoteMessage {
    pub id: Uuid,
    pub task_id: i64,
    pub user_id: Uuid,
    pub agent: String,
    pub role: String,
    pub content: String,
    pub created_at: DateTime<Utc>,
}
#[derive(Serialize)]
pub struct CreateMessage<'a> {
    pub agent: &'a str,
    pub role: &'a str,
    pub content: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct AgentSession {
    pub id: Uuid,
    pub task_id: i64,
    pub user_id: Uuid,
    pub agent: String,
    pub started_at: DateTime<Utc>,
    pub ended_at: Option<DateTime<Utc>>,
}

#[derive(Serialize)]
struct StartSession<'a> {
    agent: &'a str,
}

#[derive(Debug, Deserialize)]
pub struct Handoff {
    pub id: Uuid,
    pub task_id: i64,
    pub from_user_id: Uuid,
    pub to_user_id: Uuid,
    pub note: String,
    pub created_at: DateTime<Utc>,
    pub accepted_at: Option<DateTime<Utc>>,
}

#[derive(Debug, Deserialize)]
pub struct User {
    pub id: Uuid,
    pub email: String,
    pub username: String,
}

#[derive(Debug, Deserialize)]
pub struct Notification {
    pub id: Uuid,
    pub task_id: Option<i64>,
    pub kind: String,
    pub content: String,
    pub read_at: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
}

#[derive(Serialize)]
struct CreateHandoff<'a> {
    to_user_id: Uuid,
    note: &'a str,
}
