use anyhow::Context;
use api::config::AppConfig;
use api::http;
use tracing_subscriber::EnvFilter;

/// Entry point. It loads and validates configuration, initializes tracing and
/// serves the GraphQL schema over HTTP. Nothing about the process is hardcoded —
/// the resolved settings are logged so a misconfiguration is visible at boot.
#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env())
        .init();

    let config = AppConfig::load().context("failed to load configuration")?;
    config.validate().context("invalid configuration")?;

    tracing::info!(
        host = %config.server.host,
        port = config.server.port,
        base_url = %config.server.base_url,
        storage = %config.storage.provider,
        auth = %config.auth.provider,
        subscriptions = %config.subscriptions.transport,
        cors_origins = ?config.cors.origins,
        db_case = %config.database.case,
        db_log = config.database.log,
        "configuration resolved",
    );

    http::serve(&config).await
}
