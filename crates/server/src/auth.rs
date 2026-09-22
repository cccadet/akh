use argon2::{
    Argon2, PasswordHash, PasswordVerifier,
    password_hash::{PasswordHasher, SaltString, rand_core::OsRng},
};
use axum::{
    Json,
    extract::{Request, State},
    http::{StatusCode, header},
    middleware::Next,
    response::Response,
};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::{
    AppState,
    error::{ApiError, ApiResult},
};

#[derive(Clone, Copy)]
pub struct CurrentUser(pub Uuid);

#[derive(Deserialize)]
pub struct RegisterRequest {
    pub email: String,
    pub username: String,
    pub password: String,
}

#[derive(Deserialize)]
pub struct LoginRequest {
    pub identity: String,
    pub password: String,
}

#[derive(Serialize)]
pub struct AuthResponse {
    pub token: String,
    pub user_id: Uuid,
}

pub async fn register(
    State(state): State<AppState>,
    Json(input): Json<RegisterRequest>,
) -> ApiResult<(StatusCode, Json<AuthResponse>)> {
    if input.email.trim().is_empty() || input.username.trim().is_empty() || input.password.len() < 8
    {
        return Err(ApiError(
            StatusCode::BAD_REQUEST,
            "email, username and an 8+ character password are required".into(),
        ));
    }
    let salt = SaltString::generate(&mut OsRng);
    let password_hash = Argon2::default()
        .hash_password(input.password.as_bytes(), &salt)?
        .to_string();
    let user_id = Uuid::new_v4();
    let inserted = sqlx::query(
        "INSERT INTO users (id, email, username, password_hash) VALUES ($1, lower($2), $3, $4)",
    )
    .bind(user_id)
    .bind(input.email.trim())
    .bind(input.username.trim())
    .bind(password_hash)
    .execute(&state.db)
    .await;
    if let Err(error) = inserted {
        if matches!(&error, sqlx::Error::Database(db) if db.is_unique_violation()) {
            return Err(ApiError(
                StatusCode::CONFLICT,
                "email or username already exists".into(),
            ));
        }
        return Err(error.into());
    }
    let token = create_token(&state, user_id).await?;
    Ok((StatusCode::CREATED, Json(AuthResponse { token, user_id })))
}

pub async fn login(
    State(state): State<AppState>,
    Json(input): Json<LoginRequest>,
) -> ApiResult<Json<AuthResponse>> {
    let row = sqlx::query_as::<_, (Uuid, String)>(
        "SELECT id, password_hash FROM users WHERE lower(email) = lower($1) OR username = $1",
    )
    .bind(input.identity.trim())
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "invalid credentials".into()))?;
    let parsed = PasswordHash::new(&row.1)?;
    if Argon2::default()
        .verify_password(input.password.as_bytes(), &parsed)
        .is_err()
    {
        return Err(ApiError(
            StatusCode::UNAUTHORIZED,
            "invalid credentials".into(),
        ));
    }
    let token = create_token(&state, row.0).await?;
    Ok(Json(AuthResponse {
        token,
        user_id: row.0,
    }))
}

async fn create_token(state: &AppState, user_id: Uuid) -> ApiResult<String> {
    let token = format!("akh_{}{}", Uuid::new_v4().simple(), Uuid::new_v4().simple());
    sqlx::query("INSERT INTO auth_tokens (token, user_id, expires_at) VALUES ($1, $2, $3)")
        .bind(&token)
        .bind(user_id)
        .bind(Utc::now() + Duration::days(30))
        .execute(&state.db)
        .await?;
    Ok(token)
}

pub async fn require_auth(
    State(state): State<AppState>,
    mut request: Request,
    next: Next,
) -> Result<Response, ApiError> {
    let token = request
        .headers()
        .get(header::AUTHORIZATION)
        .and_then(|value| value.to_str().ok())
        .and_then(|value| value.strip_prefix("Bearer "))
        .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "missing bearer token".into()))?;
    let user_id = sqlx::query_scalar::<_, Uuid>(
        "SELECT user_id FROM auth_tokens WHERE token = $1 AND expires_at > CURRENT_TIMESTAMP",
    )
    .bind(token)
    .fetch_optional(&state.db)
    .await?
    .ok_or_else(|| ApiError(StatusCode::UNAUTHORIZED, "invalid or expired token".into()))?;
    request.extensions_mut().insert(CurrentUser(user_id));
    Ok(next.run(request).await)
}
