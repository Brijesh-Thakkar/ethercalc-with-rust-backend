pub mod amazon;
pub mod app;
pub mod auth;
pub mod business;
pub mod download;
pub mod dropbox;
pub mod email;
pub mod finance;
pub mod image;
pub mod import;
pub mod inapp;
pub mod insert;
pub mod pdf;
pub mod restore;
pub mod run_as;
pub mod save;
pub mod user_sheet;
pub mod webapp;

use crate::session_token::{unix_now, SessionClaims, SessionSigner, SESSION_COOKIE};
use crate::AppState;
use axum::{
    extract::State,
    response::{Html, IntoResponse, Json, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde_json::json;

pub async fn health_check(State(state): State<AppState>) -> Json<serde_json::Value> {
    Json(json!({
        "status": "healthy",
        "service": "rust-backend-infra-calc",
        "storage": state.config.storage_backend,
        "templates_loaded": true,
    }))
}

pub async fn home(State(state): State<AppState>, jar: CookieJar) -> Result<Redirect, Redirect> {
    if get_current_user_id(&jar, &state.session).is_some() {
        return Ok(Redirect::to("/dashboard"));
    }
    Ok(Redirect::to("/login"))
}

#[allow(dead_code)]
pub async fn home_page(State(_state): State<AppState>) -> impl IntoResponse {
    let html = std::fs::read_to_string("web/templates/home.html")
        .unwrap_or_else(|_| "<h1>Welcome to TouchCalc</h1>".to_string());
    Html(html)
}

#[allow(dead_code)]
pub async fn test_page() -> impl IntoResponse {
    let html = std::fs::read_to_string("web/templates/test-page.html").unwrap_or_else(|_| {
        "<h1>Test Page Not Found</h1><p>Please ensure web/templates/test-page.html exists</p>"
            .to_string()
    });
    Html(html)
}

pub fn current_session(jar: &CookieJar, signer: &SessionSigner) -> Option<SessionClaims> {
    let cookie = jar.get(SESSION_COOKIE)?;
    signer.verify(cookie.value(), unix_now())
}

pub fn get_current_user(jar: &CookieJar, signer: &SessionSigner) -> Option<String> {
    current_session(jar, signer).map(|c| c.email)
}

pub fn get_current_user_id(jar: &CookieJar, signer: &SessionSigner) -> Option<uuid::Uuid> {
    current_session(jar, signer).map(|c| c.user_id)
}

/// Issues the signed session cookie for a freshly authenticated user.
pub fn create_user_cookie(
    signer: &SessionSigner,
    email: &str,
    user_id: uuid::Uuid,
) -> Cookie<'static> {
    signer.cookie(user_id, email, unix_now())
}

/// Expires the session cookie, plus the unsigned cookies used by earlier versions.
pub fn clear_user_cookie() -> [Cookie<'static>; 3] {
    [SESSION_COOKIE, "user", "user_id"].map(|name| {
        Cookie::build((name, ""))
            .path("/")
            .http_only(true)
            .max_age(time::Duration::seconds(0))
            .build()
    })
}
