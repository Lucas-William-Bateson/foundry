pub mod workos;

pub use workos::WorkOsProvider;

use anyhow::Result;
use async_trait::async_trait;
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
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tracing::{error, info, warn};

use crate::{config::AuthConfig, AppState};

const SESSION_COOKIE_NAME: &str = "foundry_session";
const STATE_COOKIE_NAME: &str = "foundry_oauth_state";

// ---------------------------------------------------------------------------
// Provider trait
// ---------------------------------------------------------------------------

/// User info returned by an OIDC provider after code exchange.
#[derive(Debug, Clone)]
pub struct AuthenticatedUser {
    pub id: String,
    pub email: String,
    pub name: Option<String>,
}

/// Trait abstracting an OIDC authentication provider.
#[async_trait]
pub trait OidcProvider: Send + Sync + 'static {
    /// Generate the authorization URL for login redirect.
    fn auth_url(&self, state: &str) -> Result<String>;

    /// Exchange an authorization code for user info.
    async fn exchange_code(&self, code: &str) -> Result<AuthenticatedUser>;
}

// ---------------------------------------------------------------------------
// Session management (provider-agnostic)
// ---------------------------------------------------------------------------

#[derive(Debug, Serialize, Deserialize)]
pub struct SessionClaims {
    pub email: String,
    pub exp: i64,
    pub iat: i64,
}

pub struct AuthState {
    pub config: AuthConfig,
    pub provider: Arc<dyn OidcProvider>,
}

impl AuthState {
    pub fn new(config: AuthConfig, provider: Arc<dyn OidcProvider>) -> Self {
        Self { config, provider }
    }

    /// Validate a session token (our own HS256 JWT).
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

// ---------------------------------------------------------------------------
// Route-level types
// ---------------------------------------------------------------------------

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

// ---------------------------------------------------------------------------
// Routes
// ---------------------------------------------------------------------------

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

    let auth_url = match auth.provider.auth_url(&oauth_state) {
        Ok(url) => url,
        Err(e) => {
            error!("Failed to build auth URL: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Authentication failed").into_response();
        }
    };

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

    // Exchange code via the provider
    let user = match auth.provider.exchange_code(&params.code).await {
        Ok(u) => u,
        Err(e) => {
            error!("Failed to exchange code: {}", e);
            return (StatusCode::INTERNAL_SERVER_ERROR, "Authentication failed").into_response();
        }
    };

    // Check if email is allowed
    if !auth.config.allowed_emails.is_empty() && !auth.config.allowed_emails.contains(&user.email)
    {
        warn!("Unauthorized email attempted login: {}", user.email);
        return (
            StatusCode::FORBIDDEN,
            "You are not authorized to access this application",
        )
            .into_response();
    }

    info!("User logged in: {}", user.email);

    // Create our own HS256 session token
    let session_token = match auth.create_session(&user.email) {
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
