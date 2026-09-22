use axum::{
    Extension, Json, Router,
    extract::{Path, Query, State},
    http::StatusCode,
    routing::{get, post},
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    auth::{CurrentUser, login, register},
    error::{ApiError, ApiResult},
};

pub fn public_router() -> Router<AppState> {
    Router::new()
        .route("/health", get(|| async { "ok" }))
        .route("/auth/register", post(register))
        .route("/auth/login", post(login))
}

pub fn protected_router() -> Router<AppState> {
    Router::new()
        .route("/projects", get(list_projects).post(create_project))
        .route("/users", get(list_users))
        .route("/tasks", get(list_tasks).post(create_task))
        .route("/tasks/{id}", get(get_task).patch(update_task))
        .route(
            "/tasks/{id}/messages",
            get(list_messages).post(create_message),
        )
        .route("/tasks/{id}/sessions", post(start_session))
        .route("/sessions/{id}", axum::routing::patch(end_session))
        .route(
            "/tasks/{id}/handoffs",
            post(create_handoff).get(list_handoffs),
        )
        .route("/handoffs/{id}/accept", post(accept_handoff))
        .route("/notifications", get(list_notifications))
}

#[derive(Serialize, sqlx::FromRow)]
struct UserSummary {
    id: Uuid,
    email: String,
    username: String,
}

async fn list_users(State(state): State<AppState>) -> ApiResult<Json<Vec<UserSummary>>> {
    Ok(Json(
        sqlx::query_as("SELECT id,email,username FROM users ORDER BY username")
            .fetch_all(&state.db)
            .await?,
    ))
}

#[derive(Serialize, sqlx::FromRow)]
struct Project {
    id: Uuid,
    name: String,
    repository_url: String,
    default_branch: String,
}

#[derive(Deserialize)]
struct CreateProject {
    name: String,
    repository_url: String,
    #[serde(default = "main_branch")]
    default_branch: String,
}

async fn list_projects(State(state): State<AppState>) -> ApiResult<Json<Vec<Project>>> {
    Ok(Json(
        sqlx::query_as(
            "SELECT id, name, repository_url, default_branch FROM projects ORDER BY name",
        )
        .fetch_all(&state.db)
        .await?,
    ))
}

async fn create_project(
    State(state): State<AppState>,
    Json(input): Json<CreateProject>,
) -> ApiResult<(StatusCode, Json<Project>)> {
    let project = sqlx::query_as("INSERT INTO projects (id, name, repository_url, default_branch) VALUES ($1, $2, $3, $4) RETURNING id, name, repository_url, default_branch")
        .bind(Uuid::new_v4()).bind(input.name).bind(input.repository_url).bind(input.default_branch)
        .fetch_one(&state.db).await?;
    Ok((StatusCode::CREATED, Json(project)))
}

#[derive(Serialize, sqlx::FromRow)]
struct Task {
    id: i64,
    project_id: Uuid,
    title: String,
    description: String,
    branch: String,
    latest_commit: Option<String>,
    status: String,
    created_by: Uuid,
    created_at: DateTime<Utc>,
}

#[derive(Deserialize)]
struct CreateTask {
    project_id: Uuid,
    title: String,
    #[serde(default)]
    description: String,
    branch: String,
}

#[derive(Deserialize)]
struct UpdateTask {
    title: Option<String>,
    description: Option<String>,
    branch: Option<String>,
    latest_commit: Option<String>,
    status: Option<String>,
}

#[derive(Deserialize)]
struct TaskFilter {
    project_id: Option<Uuid>,
}

async fn list_tasks(
    State(state): State<AppState>,
    Query(filter): Query<TaskFilter>,
) -> ApiResult<Json<Vec<Task>>> {
    let tasks = if let Some(project_id) = filter.project_id {
        sqlx::query_as("SELECT * FROM tasks WHERE project_id = $1 ORDER BY id DESC")
            .bind(project_id)
            .fetch_all(&state.db)
            .await?
    } else {
        sqlx::query_as("SELECT * FROM tasks ORDER BY id DESC")
            .fetch_all(&state.db)
            .await?
    };
    Ok(Json(tasks))
}

async fn create_task(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Json(input): Json<CreateTask>,
) -> ApiResult<(StatusCode, Json<Task>)> {
    let task = sqlx::query_as("INSERT INTO tasks (project_id, title, description, branch, created_by) VALUES ($1,$2,$3,$4,$5) RETURNING *")
        .bind(input.project_id).bind(input.title).bind(input.description).bind(input.branch).bind(user.0)
        .fetch_one(&state.db).await?;
    Ok((StatusCode::CREATED, Json(task)))
}

async fn get_task(State(state): State<AppState>, Path(id): Path<i64>) -> ApiResult<Json<Task>> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM tasks WHERE id=$1")
            .bind(id)
            .fetch_optional(&state.db)
            .await?
            .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "task not found".into()))?,
    ))
}

async fn update_task(
    State(state): State<AppState>,
    Path(id): Path<i64>,
    Json(input): Json<UpdateTask>,
) -> ApiResult<Json<Task>> {
    let task = sqlx::query_as("UPDATE tasks SET title=COALESCE($2,title), description=COALESCE($3,description), branch=COALESCE($4,branch), latest_commit=COALESCE($5,latest_commit), status=COALESCE($6,status), updated_at=CURRENT_TIMESTAMP WHERE id=$1 RETURNING *")
        .bind(id).bind(input.title).bind(input.description).bind(input.branch).bind(input.latest_commit).bind(input.status)
        .fetch_optional(&state.db).await?.ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "task not found".into()))?;
    Ok(Json(task))
}

#[derive(Serialize, sqlx::FromRow)]
struct Message {
    id: Uuid,
    task_id: i64,
    user_id: Uuid,
    agent: String,
    role: String,
    content: String,
    created_at: DateTime<Utc>,
}
#[derive(Deserialize)]
struct CreateMessage {
    agent: String,
    role: String,
    content: String,
}

async fn list_messages(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<Message>>> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM messages WHERE task_id=$1 ORDER BY created_at,id")
            .bind(id)
            .fetch_all(&state.db)
            .await?,
    ))
}
async fn create_message(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<i64>,
    Json(input): Json<CreateMessage>,
) -> ApiResult<(StatusCode, Json<Message>)> {
    if !matches!(input.role.as_str(), "user" | "assistant") {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "role must be user or assistant".into(),
        ));
    }
    let message = sqlx::query_as("INSERT INTO messages (id,task_id,user_id,agent,role,content) VALUES ($1,$2,$3,$4,$5,$6) RETURNING *")
        .bind(Uuid::new_v4()).bind(id).bind(user.0).bind(input.agent).bind(input.role).bind(input.content).fetch_one(&state.db).await?;
    Ok((StatusCode::CREATED, Json(message)))
}

#[derive(Serialize, sqlx::FromRow)]
struct AgentSession {
    id: Uuid,
    task_id: i64,
    user_id: Uuid,
    agent: String,
    started_at: DateTime<Utc>,
    ended_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
struct StartSession {
    agent: String,
}
async fn start_session(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<i64>,
    Json(input): Json<StartSession>,
) -> ApiResult<(StatusCode, Json<AgentSession>)> {
    let session = sqlx::query_as(
        "INSERT INTO agent_sessions (id,task_id,user_id,agent) VALUES ($1,$2,$3,$4) RETURNING *",
    )
    .bind(Uuid::new_v4())
    .bind(id)
    .bind(user.0)
    .bind(input.agent)
    .fetch_one(&state.db)
    .await?;
    Ok((StatusCode::CREATED, Json(session)))
}
async fn end_session(
    State(state): State<AppState>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<AgentSession>> {
    Ok(Json(
        sqlx::query_as(
            "UPDATE agent_sessions SET ended_at=CURRENT_TIMESTAMP WHERE id=$1 RETURNING *",
        )
        .bind(id)
        .fetch_optional(&state.db)
        .await?
        .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "session not found".into()))?,
    ))
}

#[derive(Serialize, sqlx::FromRow)]
struct Handoff {
    id: Uuid,
    task_id: i64,
    from_user_id: Uuid,
    to_user_id: Uuid,
    note: String,
    created_at: DateTime<Utc>,
    accepted_at: Option<DateTime<Utc>>,
}
#[derive(Deserialize)]
struct CreateHandoff {
    to_user_id: Uuid,
    note: String,
}
async fn create_handoff(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<i64>,
    Json(input): Json<CreateHandoff>,
) -> ApiResult<(StatusCode, Json<Handoff>)> {
    let mut transaction = state.db.begin().await?;
    let handoff = sqlx::query_as("INSERT INTO handoffs (id,task_id,from_user_id,to_user_id,note) VALUES ($1,$2,$3,$4,$5) RETURNING *")
        .bind(Uuid::new_v4()).bind(id).bind(user.0).bind(input.to_user_id).bind(&input.note).fetch_one(&mut *transaction).await?;
    sqlx::query("INSERT INTO notifications (id,user_id,task_id,kind,content) VALUES ($1,$2,$3,'handoff',$4)")
        .bind(Uuid::new_v4()).bind(input.to_user_id).bind(id).bind(input.note).execute(&mut *transaction).await?;
    transaction.commit().await?;
    Ok((StatusCode::CREATED, Json(handoff)))
}
async fn list_handoffs(
    State(state): State<AppState>,
    Path(id): Path<i64>,
) -> ApiResult<Json<Vec<Handoff>>> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM handoffs WHERE task_id=$1 ORDER BY created_at")
            .bind(id)
            .fetch_all(&state.db)
            .await?,
    ))
}

async fn accept_handoff(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
    Path(id): Path<Uuid>,
) -> ApiResult<Json<Handoff>> {
    let handoff = sqlx::query_as(
        "UPDATE handoffs SET accepted_at=CURRENT_TIMESTAMP WHERE id=$1 AND to_user_id=$2 RETURNING *",
    )
    .bind(id)
    .bind(user.0)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError(StatusCode::NOT_FOUND, "handoff not found".into()))?;
    Ok(Json(handoff))
}

#[derive(Serialize, sqlx::FromRow)]
struct Notification {
    id: Uuid,
    user_id: Uuid,
    task_id: Option<i64>,
    kind: String,
    content: String,
    read_at: Option<DateTime<Utc>>,
    created_at: DateTime<Utc>,
}

async fn list_notifications(
    State(state): State<AppState>,
    Extension(user): Extension<CurrentUser>,
) -> ApiResult<Json<Vec<Notification>>> {
    Ok(Json(
        sqlx::query_as("SELECT * FROM notifications WHERE user_id=$1 ORDER BY created_at DESC")
            .bind(user.0)
            .fetch_all(&state.db)
            .await?,
    ))
}

fn main_branch() -> String {
    "main".into()
}
