use anyhow::{anyhow, Result};
use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{IntoResponse, Json, Redirect, Response},
    routing::get,
    Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar, SameSite};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::Rng;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::{collections::HashMap, sync::Arc, time::Duration};
use tokio::sync::RwLock;
use tracing::{error, info, warn};

use crate::{config::AuthConfig, AppState};

const SESSION_COOKIE_NAME: &str = "foundry_session";
const STATE_COOKIE_NAME: &str = "foundry_oauth_state";

#[derive(Clone)]
pub struct AuthState {
    pub config: AuthConfig,
    pub oidc_config: OidcConfig,
    pub jwks: Arc<RwLock<Jwks>>,
    pub sessions: Arc<RwLock<HashMap<String, Session>>>,
    http_client: Client,
}

#[derive(Clone, Debug, Deserialize)]
pub struct OidcConfig {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
    pub userinfo_endpoint: String,
    pub jwks_uri: String,
    pub issuer: String,
}

#[derive(Clone, Debug, Default)]
pub struct Jwks {
    pub keys: Vec<JwkKey>,
}

#[derive(Clone, Debug, Deserialize)]
pub struct JwkKey {
    pub kid: String,
    pub kty: String,
    pub alg: Option<String>,
    pub n: Option<String>,
    pub e: Option<String>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Session {
    pub email: String,
    pub name: Option<String>,
    pub created_at: i64,
    pub expires_at: i64,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct TokenResponse {
    pub access_token: String,
    pub id_token: Option<String>,
    pub token_type: String,
    pub expires_in: Option<u64>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct IdTokenClaims {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Deserialize)]
pub struct UserInfo {
    pub sub: String,
    pub email: Option<String>,
    pub name: Option<String>,
    pub preferred_username: Option<String>,
}

#[derive(Deserialize)]
pub struct AuthCallback {
    pub code: String,
    pub state: String,
}

#[derive(Serialize)]
pub struct AuthStatus {
    pub authenticated: bool,
    pub email: Option<String>,
    pub name: Option<String>,
}

impl AuthState {
    pub async fn new(config: AuthConfig) -> Result<Self> {
        let http_client = Client::builder()
            .timeout(Duration::from_secs(10))
            .build()?;

        // Fetch OIDC discovery document
        let discovery_url = format!("{}/.well-known/openid-configuration", config.issuer_url);
        info!("Fetching OIDC config from {}", discovery_url);

        let oidc_config: OidcConfig = http_client
            .get(&discovery_url)
            .send()
            .await?
            .json()
            .await?;

        info!("OIDC config loaded: issuer={}", oidc_config.issuer);

        // Fetch JWKS
        let jwks_response: serde_json::Value = http_client
            .get(&oidc_config.jwks_uri)
            .send()
            .await?
            .json()
            .await?;

        let keys: Vec<JwkKey> = serde_json::from_value(
            jwks_response.get("keys").cloned().unwrap_or_default(),
        )
        .unwrap_or_default();

        info!("Loaded {} JWKS keys", keys.len());

        Ok(Self {
            config,
            oidc_config,
            jwks: Arc::new(RwLock::new(Jwks { keys })),
            sessions: Arc::new(RwLock::new(HashMap::new())),
            http_client,
        })
    }

    pub async fn validate_session(&self, session_id: &str) -> Option<Session> {
        let sessions = self.sessions.read().await;
        if let Some(session) = sessions.get(session_id) {
            let now = chrono::Utc::now().timestamp();
            if session.expires_at > now {
                // Check allowed emails if configured
                if self.config.allowed_emails.is_empty() 
                    || self.config.allowed_emails.contains(&session.email) 
                {
                    return Some(session.clone());
                }
            }
        }
        None
    }

    fn generate_session_id(&self) -> String {
        let random_bytes: [u8; 32] = rand::thread_rng().gen();
        URL_SAFE_NO_PAD.encode(random_bytes)
    }

    fn generate_state(&self) -> String {
        let random_bytes: [u8; 16] = rand::thread_rng().gen();
        URL_SAFE_NO_PAD.encode(random_bytes)
    }
}

pub fn router() -> Router<Arc<AppState>> {
    Router::new()
        .route("/auth/login", get(login))
        .route("/auth/callback", get(callback))
        .route("/auth/logout", get(logout))
        .route("/auth/status", get(status))
}

async fn login(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    let auth = match &state.auth {
        Some(auth) => auth,
        None => {
            return (
                StatusCode::SERVICE_UNAVAILABLE,
                "Auth not configured",
            )
                .into_response()
        }
    };

    let oauth_state = auth.generate_state();
    
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}",
        auth.oidc_config.authorization_endpoint,
        urlencoding::encode(&auth.config.client_id),
        urlencoding::encode(&auth.config.redirect_url),
        urlencoding::encode(&oauth_state),
    );

    let state_cookie = Cookie::build((STATE_COOKIE_NAME, oauth_state))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::minutes(10))
        .build();

    (jar.add(state_cookie), Redirect::to(&auth_url)).into_response()
}

async fn callback(
    State(state): State<Arc<AppState>>,
    Query(params): Query<AuthCallback>,
    jar: CookieJar,
) -> impl IntoResponse {
    let auth = match &state.auth {
        Some(auth) => auth,
        None => {
            return (StatusCode::SERVICE_UNAVAILABLE, "Auth not configured").into_response()
        }
    };

    // Verify state
    let state_cookie = jar.get(STATE_COOKIE_NAME);
    if state_cookie.map(|c| c.value()) != Some(&params.state) {
        warn!("OAuth state mismatch");
        return (StatusCode::BAD_REQUEST, "Invalid state").into_response();
    }

    // Exchange code for token
    let token_response = match exchange_code(auth, &params.code).await {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to exchange code: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Token exchange failed").into_response();
        }
    };

    // Get user info
    let user_info = match get_user_info(auth, &token_response.access_token).await {
        Ok(u) => u,
        Err(e) => {
            error!("Failed to get user info: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Failed to get user info").into_response();
        }
    };

    let email = user_info.email.unwrap_or_else(|| user_info.sub.clone());

    // Check if email is allowed
    if !auth.config.allowed_emails.is_empty() && !auth.config.allowed_emails.contains(&email) {
        warn!("Unauthorized email attempted login: {}", email);
        return (StatusCode::FORBIDDEN, "You are not authorized to access this application").into_response();
    }

    info!("User logged in: {}", email);

    // Create session
    let session_id = auth.generate_session_id();
    let session = Session {
        email: email.clone(),
        name: user_info.name.or(user_info.preferred_username),
        created_at: chrono::Utc::now().timestamp(),
        expires_at: chrono::Utc::now().timestamp() + 86400 * 7, // 7 days
    };

    {
        let mut sessions = auth.sessions.write().await;
        sessions.insert(session_id.clone(), session);
    }

    // Set session cookie
    let session_cookie = Cookie::build((SESSION_COOKIE_NAME, session_id))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    // Clear state cookie
    let clear_state = Cookie::build((STATE_COOKIE_NAME, ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build();

    (
        jar.add(session_cookie).add(clear_state),
        Redirect::to("/"),
    )
        .into_response()
}

async fn logout(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    if let Some(auth) = &state.auth {
        if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
            let mut sessions = auth.sessions.write().await;
            sessions.remove(session_cookie.value());
        }
    }

    let clear_session = Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .max_age(time::Duration::ZERO)
        .build();

    (jar.add(clear_session), Redirect::to("/")).into_response()
}

async fn status(State(state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    // If auth is not configured, always return authenticated
    let auth = match &state.auth {
        Some(auth) => auth,
        None => {
            return Json(AuthStatus {
                authenticated: true,
                email: None,
                name: None,
            })
        }
    };

    // Check for valid session
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        let sessions = auth.sessions.read().await;
        if let Some(session) = sessions.get(session_cookie.value()) {
            let now = chrono::Utc::now().timestamp();
            if session.expires_at > now {
                if auth.config.allowed_emails.is_empty()
                    || auth.config.allowed_emails.contains(&session.email)
                {
                    return Json(AuthStatus {
                        authenticated: true,
                        email: Some(session.email.clone()),
                        name: session.name.clone(),
                    });
                }
            }
        }
    }

    Json(AuthStatus {
        authenticated: false,
        email: None,
        name: None,
    })
}

async fn exchange_code(auth: &AuthState, code: &str) -> Result<TokenResponse> {
    let params = [
        ("grant_type", "authorization_code"),
        ("code", code),
        ("redirect_uri", &auth.config.redirect_url),
        ("client_id", &auth.config.client_id),
        ("client_secret", &auth.config.client_secret),
    ];

    let response = auth
        .http_client
        .post(&auth.oidc_config.token_endpoint)
        .form(&params)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Token exchange failed: {}", error_text));
    }

    Ok(response.json().await?)
}

async fn get_user_info(auth: &AuthState, access_token: &str) -> Result<UserInfo> {
    let response = auth
        .http_client
        .get(&auth.oidc_config.userinfo_endpoint)
        .bearer_auth(access_token)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!("User info request failed: {}", error_text));
    }

    Ok(response.json().await?)
}

// Middleware to check authentication
#[allow(dead_code)]
pub async fn require_auth(
    State(state): State<Arc<AppState>>,
    jar: CookieJar,
    request: axum::http::Request<axum::body::Body>,
    next: axum::middleware::Next,
) -> Response {
    // If auth is not configured, allow all requests
    let auth = match &state.auth {
        Some(auth) => auth,
        None => return next.run(request).await,
    };

    // Check for valid session
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        if auth.validate_session(session_cookie.value()).await.is_some() {
            return next.run(request).await;
        }
    }

    // Not authenticated - return 401 for API requests, redirect for pages
    let path = request.uri().path();
    if path.starts_with("/api/") {
        return (StatusCode::UNAUTHORIZED, "Authentication required").into_response();
    }

    // For page requests, redirect to login
    Redirect::to("/auth/login").into_response()
}
