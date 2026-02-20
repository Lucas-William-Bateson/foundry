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
use std::{sync::Arc, time::Duration};
use tracing::{error, info, warn};

use crate::{config::AuthConfig, AppState};

const SESSION_COOKIE_NAME: &str = "foundry_session";
const STATE_COOKIE_NAME: &str = "foundry_oauth_state";

#[derive(Clone)]
pub struct AuthState {
    pub config: AuthConfig,
    pub oidc_config: OidcConfig,
    http_client: Client,
}

#[derive(Clone, Debug)]
pub struct OidcConfig {
    pub authorization_endpoint: String,
    pub token_endpoint: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub email: String,
    pub exp: i64,
    pub iat: i64,
}

#[derive(Debug, Deserialize)]
pub struct WorkOsAuthResponse {
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub user: WorkOsUser,
}

#[derive(Debug, Deserialize)]
pub struct WorkOsUser {
    pub id: String,
    pub email: String,
    pub first_name: Option<String>,
    pub last_name: Option<String>,
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

        let oidc_config = OidcConfig {
            authorization_endpoint: "https://api.workos.com/user_management/authorize".to_string(),
            token_endpoint: "https://api.workos.com/user_management/authenticate".to_string(),
        };

        info!("WorkOS auth initialised (client_id={})", config.client_id);

        Ok(Self {
            config,
            oidc_config,
            http_client,
        })
    }

    /// Validate a session token (our own HS256 JWT, not WorkOS's token).
    pub fn validate_session(&self, token: &str) -> Option<SessionClaims> {
        use jsonwebtoken::{decode, Algorithm, DecodingKey, Validation};

        let key = DecodingKey::from_secret(self.config.cookie_secret.as_bytes());
        let mut validation = Validation::new(Algorithm::HS256);
        validation.validate_aud = false;

        match decode::<SessionClaims>(token, &key, &validation) {
            Ok(data) => {
                let claims = data.claims;
                if !self.config.allowed_emails.is_empty()
                    && !self.config.allowed_emails.contains(&claims.email)
                {
                    warn!("Session email not in allowed list: {}", claims.email);
                    return None;
                }
                Some(claims)
            }
            Err(e) => {
                warn!("Session token invalid: {}", e);
                None
            }
        }
    }

    /// Create a signed session token for the given email (7-day expiry).
    pub fn create_session(&self, email: &str) -> Result<String> {
        use jsonwebtoken::{encode, Algorithm, EncodingKey, Header};
        use std::time::{SystemTime, UNIX_EPOCH};

        let now = SystemTime::now().duration_since(UNIX_EPOCH)?.as_secs() as i64;
        let claims = SessionClaims {
            email: email.to_string(),
            iat: now,
            exp: now + 7 * 24 * 3600,
        };

        let key = EncodingKey::from_secret(self.config.cookie_secret.as_bytes());
        Ok(encode(&Header::new(Algorithm::HS256), &claims, &key)?)
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

    let oauth_state: String = {
        let b: [u8; 16] = rand::thread_rng().gen();
        URL_SAFE_NO_PAD.encode(b)
    };
    
    let auth_url = format!(
        "{}?client_id={}&redirect_uri={}&response_type=code&scope=openid%20email%20profile&state={}&provider=authkit",
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

    // Exchange code — WorkOS returns user info directly
    let workos_response = match exchange_code(auth, &params.code).await {
        Ok(r) => r,
        Err(e) => {
            error!("Failed to exchange code: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Authentication failed").into_response();
        }
    };

    let email = workos_response.user.email.clone();
    let first_name = workos_response.user.first_name;
    let last_name = workos_response.user.last_name;
    let _name = first_name
        .map(|f| format!("{} {}", f, last_name.unwrap_or_default()))
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty());

    // Check if email is allowed
    if !auth.config.allowed_emails.is_empty() && !auth.config.allowed_emails.contains(&email) {
        warn!("Unauthorized email attempted login: {}", email);
        return (StatusCode::FORBIDDEN, "You are not authorized to access this application").into_response();
    }

    info!("User logged in: {}", email);

    // Create our own HS256 session token — avoids WorkOS JWT validation complexity
    let session_token = match auth.create_session(&email) {
        Ok(t) => t,
        Err(e) => {
            error!("Failed to create session token: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Authentication failed").into_response();
        }
    };

    let session_cookie = Cookie::build((SESSION_COOKIE_NAME, session_token))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::days(7))
        .build();

    // Clear state cookie
    let clear_state = Cookie::build((STATE_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
        .max_age(time::Duration::ZERO)
        .build();

    (
        jar.add(session_cookie).add(clear_state),
        Redirect::to("/"),
    )
        .into_response()
}

async fn logout(State(_state): State<Arc<AppState>>, jar: CookieJar) -> impl IntoResponse {
    let clear_session = Cookie::build((SESSION_COOKIE_NAME, ""))
        .path("/")
        .http_only(true)
        .secure(true)
        .same_site(SameSite::Lax)
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

    // Validate session cookie
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        if let Some(claims) = auth.validate_session(session_cookie.value()) {
            return Json(AuthStatus {
                authenticated: true,
                email: Some(claims.email),
                name: None,
            });
        }
    }

    Json(AuthStatus {
        authenticated: false,
        email: None,
        name: None,
    })
}

async fn exchange_code(auth: &AuthState, code: &str) -> Result<WorkOsAuthResponse> {
    let body = serde_json::json!({
        "client_id": auth.config.client_id,
        "client_secret": auth.config.client_secret,
        "code": code,
        "grant_type": "authorization_code"
    });

    let response = auth
        .http_client
        .post(&auth.oidc_config.token_endpoint)
        .json(&body)
        .send()
        .await?;

    if !response.status().is_success() {
        let error_text = response.text().await.unwrap_or_default();
        return Err(anyhow!("Token exchange failed: {}", error_text));
    }

    Ok(response.json().await?)
}

// Middleware to check authentication
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

    // Validate session cookie
    if let Some(session_cookie) = jar.get(SESSION_COOKIE_NAME) {
        if auth.validate_session(session_cookie.value()).is_some() {
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
