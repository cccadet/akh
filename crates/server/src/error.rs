use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;

pub struct ApiError(pub StatusCode, pub String);

impl IntoResponse for ApiError {
    fn into_response(self) -> axum::response::Response {
        (self.0, Json(json!({ "error": self.1 }))).into_response()
    }
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        tracing::error!(%error, "database error");
        Self(StatusCode::INTERNAL_SERVER_ERROR, "database error".into())
    }
}

impl From<argon2::password_hash::Error> for ApiError {
    fn from(error: argon2::password_hash::Error) -> Self {
        tracing::error!(%error, "password hashing error");
        Self(
            StatusCode::INTERNAL_SERVER_ERROR,
            "authentication error".into(),
        )
    }
}

pub type ApiResult<T> = Result<T, ApiError>;
