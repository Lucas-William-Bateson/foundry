pub mod api;
pub mod config;
pub mod domain;
pub mod infrastructure;
pub mod runtime;

use anyhow::Result;
use axum::Router;
use sqlx::sqlite::SqlitePoolOptions;
use std::sync::Arc;
use tokio::net::TcpListener;
use tower_http::trace::TraceLayer;
use tracing::info;
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt};

use crate::api::auth::AuthState;
use crate::infrastructure::cloudflare::{CloudflareConfig, CloudflareTunnel};
use crate::config::Config;

async fn security_headers(request: axum::http::Request<axum::body::Body>, next: axum::middleware::Next) -> impl axum::response::IntoResponse {
    let mut response = next.run(request).await;
    let headers = response.headers_mut();
    headers.insert("X-Frame-Options", axum::http::HeaderValue::from_static("DENY"));
    headers.insert("X-Content-Type-Options", axum::http::HeaderValue::from_static("nosniff"));
    headers.insert("Referrer-Policy", axum::http::HeaderValue::from_static("strict-origin-when-cross-origin"));
    headers.insert("Permissions-Policy", axum::http::HeaderValue::from_static("geolocation=(), microphone=(), camera=()"));
    response
}

pub struct AppState {
    pub db: sqlx::SqlitePool,
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

    foundry_core::bootstrap_vault_secrets("foundry/prod").await?;

    let config = Config::from_env()?;
    info!("Starting foundryd on {}", config.bind_addr);

    let db = SqlitePoolOptions::new()
        .max_connections(5)
        .connect(&config.database_url)
        .await?;

    info!("Connected to database");

    // Run migrations automatically
    info!("Running database migrations...");
    sqlx::migrate!("../../migrations_sqlite")
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
        domain::scheduler::run_scheduler(db_pool).await;
    });

    // Initialize GitHub App for the built-in worker
    let github_app = if config.has_github_app() {
        info!("GitHub App authentication enabled for built-in worker");
        match infrastructure::github_app::GitHubApp::new(
            config.github_app_id.clone().unwrap(),
            config.github_installation_id.clone().unwrap(),
            config.github_private_key.as_ref().unwrap(),
        ) {
            Ok(app) => Some(Arc::new(app)),
            Err(e) => {
                tracing::warn!("Failed to initialize GitHub App: {} — worker will clone public repos only", e);
                None
            }
        }
    } else {
        tracing::warn!("GitHub App not configured — built-in worker will clone public repos only");
        None
    };

    // Initialize auth if enabled
    let auth = if let Some(auth_config) = &config.auth {
        info!("Initializing OIDC authentication...");
        match api::auth::WorkOsProvider::new(auth_config) {
            Ok(provider) => {
                let provider: Arc<dyn api::auth::OidcProvider> = Arc::new(provider);
                let auth_state = AuthState::new(auth_config.clone(), provider);
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

    // Agent watchdog disabled — worker is now built into this binary
    // runtime::watchdog::start_agent_watchdog();

    // Start the built-in worker
    let worker_disabled = std::env::var("FOUNDRY_WORKER_DISABLED")
        .map(|v| v == "1" || v.to_lowercase() == "true")
        .unwrap_or(false);
    if !worker_disabled {
        let worker_state = state.clone();
        let worker_github_app = github_app.clone();
        tokio::spawn(async move {
            runtime::worker::run_worker(worker_state, worker_github_app).await;
        });
        info!("Built-in worker started");
    } else {
        info!("Built-in worker disabled (FOUNDRY_WORKER_DISABLED=true)");
    }

    // Build the router with optional auth protection
    let mut app = Router::new()
        .merge(api::webhook::router())
        .merge(api::health::router());

    // Add auth routes if auth is enabled
    if state.auth.is_some() {
        let protected = Router::new()
            .merge(api::frontend::api_router())
            .route_layer(axum::middleware::from_fn_with_state(state.clone(), api::auth::require_auth));
        let agent = Router::new()
            .merge(api::agent::router())
            .route_layer(axum::middleware::from_fn_with_state(state.clone(), api::agent::require_agent_auth));
        app = app
            .merge(agent)
            .merge(protected)
            .merge(api::frontend::static_router()) // public: login page must load before session exists
            .merge(api::auth::router());
    } else {
        let agent = Router::new()
            .merge(api::agent::router())
            .route_layer(axum::middleware::from_fn_with_state(state.clone(), api::agent::require_agent_auth));
        app = app
            .merge(api::frontend::router())
            .merge(agent);
    }

    let app = app
        .layer(axum::middleware::from_fn(security_headers))
        .layer(TraceLayer::new_for_http())
        .with_state(state.clone());

    let listener = TcpListener::bind(&state.config.bind_addr).await?;
    info!("Listening on {}", state.config.bind_addr);

    axum::serve(listener, app).await?;

    Ok(())
}
