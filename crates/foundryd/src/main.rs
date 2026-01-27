mod auth;
mod cloudflare;
mod config;
mod db;
mod routes;
mod scheduler;

use anyhow::Result;
use axum::Router;
use sqlx::postgres::PgPoolOptions;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::auth::AuthState;
use crate::cloudflare::{CloudflareConfig, CloudflareTunnel};
use crate::config::Config;

pub struct AppState {
    pub db: sqlx::PgPool,
    pub config: Config,
    pub auth: Option<AuthState>,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "foundryd=debug,tower_http=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();

    let config = Config::from_env()?;
    info!("Starting foundryd on {}", config.bind_addr);

    let db = PgPoolOptions::new()
        .max_connections(10)
        .connect(&config.database_url)
        .await?;

    info!("Connected to database");

    // Run migrations automatically
    info!("Running database migrations...");
    sqlx::migrate!("../../migrations")
        .run(&db)
        .await?;
    info!("Migrations complete");

    let _tunnel = if let Some(tunnel_config) = &config.tunnel {
        info!("Starting Cloudflare tunnel...");
        let cf_config = CloudflareConfig {
            account_id: tunnel_config.cf_account_id.clone(),
            api_token: tunnel_config.cf_api_token.clone(),
            zone_id: tunnel_config.cf_zone_id.clone(),
            tunnel_name: tunnel_config.tunnel_name.clone(),
            domain: tunnel_config.domain.clone(),
            local_port: config.bind_port,
        };
        let tunnel = CloudflareTunnel::start(cf_config).await?;
        info!("========================================");
        info!("Tunnel Domain: {}", tunnel.domain);
        info!("Webhook URL: {}", tunnel.webhook_url());
        info!("========================================");
        info!("Configure this webhook URL in your GitHub org settings");
        Some(tunnel)
    } else {
        None
    };

    let db_pool = Arc::new(db.clone());
    tokio::spawn(async move {
        scheduler::run_scheduler(db_pool).await;
    });

    // Initialize auth if enabled
    let auth = if let Some(auth_config) = &config.auth {
        info!("Initializing OIDC authentication...");
        match AuthState::new(auth_config.clone()).await {
            Ok(auth_state) => {
                info!("OIDC authentication initialized successfully");
                Some(auth_state)
            }
            Err(e) => {
                tracing::error!("Failed to initialize OIDC auth: {}. Running without auth.", e);
                None
            }
        }
    } else {
        info!("Authentication disabled");
        None
    };

    let state = Arc::new(AppState { db, config, auth });

    // Build the router with optional auth protection
    let mut app = Router::new()
        .merge(routes::frontend::router())
        .merge(routes::webhook::router())
        .merge(routes::agent::router())
        .merge(routes::health::router());
    
    // Add auth routes if auth is enabled
    if state.auth.is_some() {
        app = app.merge(auth::router());
    }
    
    let app = app
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = TcpListener::bind(&state.config.bind_addr).await?;
    info!("Listening on {}", state.config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
