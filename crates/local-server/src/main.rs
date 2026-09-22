use std::{net::SocketAddr, str::FromStr};

use anyhow::{Context, Result};
use sqlx::sqlite::SqliteConnectOptions;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "akh_local_server=info,tower_http=info".into()),
        )
        .init();

    let database_url = std::env::var("DATABASE_URL").unwrap_or_else(|_| "sqlite://akh.db".into());
    let options = SqliteConnectOptions::from_str(&database_url)
        .context("DATABASE_URL must be a valid SQLite URL")?;
    let bind = std::env::var("AKH_BIND").unwrap_or_else(|_| "127.0.0.1:3000".into());
    let address: SocketAddr = bind.parse().context("AKH_BIND must be a socket address")?;
    akh_local_server::serve(options, address)
        .await
        .context("local server stopped")
}
