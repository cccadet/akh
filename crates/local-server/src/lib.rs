#[path = "../../server/src/auth.rs"]
mod auth;
#[path = "../../server/src/error.rs"]
mod error;
#[path = "../../server/src/routes.rs"]
mod routes;

use std::net::SocketAddr;

use anyhow::Result;
use axum::{Router, middleware};
use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use tower_http::{cors::CorsLayer, trace::TraceLayer};
use tracing::info;

use crate::auth::require_auth;

#[derive(Clone)]
pub struct AppState {
    pub db: sqlx::SqlitePool,
}

pub async fn serve(options: SqliteConnectOptions, address: SocketAddr) -> Result<()> {
    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect_with(options.create_if_missing(true).foreign_keys(true))
        .await?;
    sqlx::migrate!("../../migrations-sqlite").run(&db).await?;
    let state = AppState { db };

    let protected = routes::protected_router()
        .layer(middleware::from_fn_with_state(state.clone(), require_auth));
    let app = Router::new()
        .merge(routes::public_router())
        .merge(protected)
        .layer(CorsLayer::permissive())
        .layer(TraceLayer::new_for_http())
        .with_state(state);

    let listener = tokio::net::TcpListener::bind(address).await?;
    info!(%address, "Akh local server listening");
    axum::serve(listener, app).await?;
    Ok(())
}
