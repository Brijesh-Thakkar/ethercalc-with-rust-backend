use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect, Response},
    Form,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Deserialize;
use serde_json::json;

use crate::{
    auth::{hash_password, verify_password},
    db::Database,
    handlers::{clear_user_cookie, create_user_cookie, get_current_user},
    reset_token,
    services::email::EmailService,
    AppState,
};

#[derive(Debug, Deserialize)]
pub struct AuthRequest {
    action: String,
    email: Option<String>,
    pwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LoginRequest {
    email: String,
    password: Option<String>,
    pwd: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct RegisterRequest {
    email: String,
    password: Option<String>,
    pwd: Option<String>,
}

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct PasswordResetQuery {
    u: String,
    d: String,
}

#[derive(Debug, Deserialize)]
pub struct PasswordResetRequest {
    email: String,
    password: String,
    #[serde(default)]
    token: String,
}

#[derive(Debug, Deserialize)]
pub struct LostPasswordRequest {
    email: String,
}

// HTML Page handlers
pub async fn login_page() -> impl IntoResponse {
    let html = std::fs::read_to_string("web/templates/login.html").unwrap_or_else(|_| {
        r#"<!DOCTYPE html>
<html><head><title>Login</title></head>
<body><h1>Login</h1>
<form method="post" action="/login">
<input type="email" name="email" placeholder="Email" required />
<input type="password" name="password" placeholder="Password" required />
<button type="submit">Login</button>
</form>
<a href="/register">Register</a>
</body></html>"#
            .to_string()
    });
    Html(html)
}

pub async fn register_page() -> impl IntoResponse {
    let html = std::fs::read_to_string("web/templates/register.html").unwrap_or_else(|_| {
        r#"<!DOCTYPE html>
<html><head><title>Register</title></head>
<body><h1>Register</h1>
<form method="post" action="/register">
<input type="email" name="email" placeholder="Email" required />
<input type="password" name="password" placeholder="Password" required />
<button type="submit">Register</button>
</form>
<a href="/login">Login</a>
</body></html>"#
            .to_string()
    });
    Html(html)
}

pub async fn lost_password_page() -> impl IntoResponse {
    let html = std::fs::read_to_string("web/templates/lostpassword.html").unwrap_or_else(|_| {
        r#"<!DOCTYPE html>
<html><head><title>Lost Password</title></head>
<body><h1>Reset Password</h1>
<form method="post">
<input type="email" name="email" placeholder="Email" required />
<button type="submit">Send Reset Link</button>
</form></body></html>"#
            .to_string()
    });
    Html(html)
}

pub async fn password_reset_page(Query(query): Query<PasswordResetQuery>) -> impl IntoResponse {
    let html = std::fs::read_to_string("web/templates/pwreset.html").unwrap_or_else(|_| {
        r#"<!DOCTYPE html>
<html><head><title>Password Reset</title></head>
<body><h1>Reset Your Password</h1>
<form method="post">
<input type="hidden" name="email" value="{{ reguser }}" />
<input type="hidden" name="token" value="{{ token }}" />
<input type="password" name="password" placeholder="New Password" required />
<button type="submit">Reset Password</button>
</form></body></html>"#
            .to_string()
    });

    let html = html
        .replace("{{ reguser }}", &html_escape(&query.u))
        .replace("{{ token }}", &html_escape(&query.d));
    Html(html)
}

fn html_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&#x27;")
}

// Handle /iauth endpoint
pub async fn handle_iauth(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(req): Form<AuthRequest>,
) -> Result<Response, StatusCode> {
    match req.action.as_str() {
        "login" => {
            let email = req.email.ok_or(StatusCode::BAD_REQUEST)?;
            let password = req.pwd.ok_or(StatusCode::BAD_REQUEST)?;
            handle_login_internal(state, jar, email, password, true).await
        }
        "register" => {
            let email = req.email.ok_or(StatusCode::BAD_REQUEST)?;
            let password = req.pwd.ok_or(StatusCode::BAD_REQUEST)?;
            handle_register_internal(state, jar, email, password, true).await
        }
        "logout" => Ok(handle_logout_internal(jar, true).into_response()),
        _ => Err(StatusCode::BAD_REQUEST),
    }
}

// Handle /login POST
pub async fn handle_login(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(req): Form<LoginRequest>,
) -> Result<Response, StatusCode> {
    let password = req.password.or(req.pwd).ok_or(StatusCode::BAD_REQUEST)?;
    handle_login_internal(state, jar, req.email, password, false).await
}

// Handle /register POST
pub async fn handle_register(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(req): Form<RegisterRequest>,
) -> Result<Response, StatusCode> {
    let password = req.password.or(req.pwd).ok_or(StatusCode::BAD_REQUEST)?;
    handle_register_internal(state, jar, req.email, password, false).await
}

// Handle /logout GET/POST
pub async fn handle_logout(jar: CookieJar) -> impl IntoResponse {
    handle_logout_internal(jar, false)
}

// Internal handlers
async fn handle_login_internal(
    state: AppState,
    jar: CookieJar,
    email: String,
    password: String,
    is_json: bool,
) -> Result<Response, StatusCode> {
    // Validate email
    if !email.contains('@') {
        if is_json {
            return Ok(Json(json!({
                "data": "usererror",
                "result": "fail"
            }))
            .into_response());
        } else {
            let html = std::fs::read_to_string("web/templates/login.html").unwrap_or_default();
            let html = html.replace("{{ error }}", "Please enter a valid email address");
            return Ok(Html(html).into_response());
        }
    }

    // Get user
    let user = match state.db.get_user_by_email(&email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            if is_json {
                return Ok(Json(json!({
                    "data": "authfail",
                    "result": "fail"
                }))
                .into_response());
            } else {
                let html = std::fs::read_to_string("web/templates/login.html").unwrap_or_default();
                let html = html.replace("{{ error }}", "User does not exist");
                return Ok(Html(html).into_response());
            }
        }
        Err(e) => {
            tracing::error!("Database error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Verify password
    let password_valid = match verify_password(&password, &user.password_hash) {
        Ok(valid) => valid,
        Err(e) => {
            tracing::error!("Password verification error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if !password_valid {
        if is_json {
            return Ok(Json(json!({
                "data": "authfail",
                "result": "fail"
            }))
            .into_response());
        } else {
            let html = std::fs::read_to_string("web/templates/login.html").unwrap_or_default();
            let html = html.replace("{{ error }}", "Invalid email or password");
            return Ok(Html(html).into_response());
        }
    }

    let [c1, c2] = create_user_cookie(&email, user.id);
    let jar = jar.add(c1).add(c2);

    if is_json {
        Ok((
            jar,
            Json(json!({
                "data": "success",
                "result": "ok"
            })),
        )
            .into_response())
    } else {
        Ok((jar, Redirect::to("/browser")).into_response())
    }
}

async fn handle_register_internal(
    state: AppState,
    jar: CookieJar,
    email: String,
    password: String,
    is_json: bool,
) -> Result<Response, StatusCode> {
    tracing::info!("Starting registration for email: {}", email);

    // Validate email
    if !email.contains('@') {
        if is_json {
            return Ok(Json(json!({
                "data": "usererror",
                "result": "fail",
                "message": "Invalid email format"
            }))
            .into_response());
        } else {
            let html = std::fs::read_to_string("web/templates/register.html").unwrap_or_default();
            let html = html.replace("{{ error }}", "Please enter a valid email address");
            return Ok(Html(html).into_response());
        }
    }

    // Check if user exists
    match state.db.get_user_by_email(&email).await {
        Ok(Some(_)) => {
            if is_json {
                return Ok(Json(json!({
                    "data": "userexists",
                    "result": "fail",
                    "message": "User already exists"
                }))
                .into_response());
            } else {
                let html =
                    std::fs::read_to_string("web/templates/register.html").unwrap_or_default();
                let html = html.replace("{{ error }}", "User already exists");
                return Ok(Html(html).into_response());
            }
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Database error checking user: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Hash password
    let password_hash = match hash_password(&password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("Password hashing error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Create user
    match state.db.create_user(&email, &password_hash).await {
        Ok(user) => {
            tracing::info!("User created successfully: {}", email);

            // TODO: Create user directories in storage
            // This would be implemented based on the storage backend

            let [c1, c2] = create_user_cookie(&email, user.id);
            let jar = jar.add(c1).add(c2);

            if is_json {
                Ok((
                    jar,
                    Json(json!({
                        "data": "success",
                        "result": "ok",
                        "message": "Registration successful"
                    })),
                )
                    .into_response())
            } else {
                Ok((jar, Redirect::to("/browser")).into_response())
            }
        }
        Err(e) => {
            tracing::error!("User creation error: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

fn handle_logout_internal(jar: CookieJar, is_json: bool) -> Response {
    tracing::info!("Logging out user");

    let [c1, c2] = clear_user_cookie();
    let jar = jar.add(c1).add(c2).remove(Cookie::from("session"));

    if is_json {
        (
            jar,
            Json(json!({
                "result": "ok"
            })),
        )
            .into_response()
    } else {
        (jar, Redirect::to("/browser")).into_response()
    }
}

// Password reset handlers
pub async fn lost_password(
    State(state): State<AppState>,
    Form(form): Form<LostPasswordRequest>,
) -> Result<Html<String>, StatusCode> {
    // Check if user exists
    match state.db.get_user_by_email(&form.email).await {
        Ok(Some(user)) => {
            let (token, stored) = reset_token::generate(reset_token::now());
            if let Err(e) = state.db.set_user_dongle(user.id, &stored).await {
                tracing::error!("Failed to store reset token: {:?}", e);
                return Err(StatusCode::INTERNAL_SERVER_ERROR);
            }

            send_reset_email(&state, &form.email, &token).await;
            tracing::info!("Password reset requested for: {}", form.email);

            let html = std::fs::read_to_string("web/templates/lostpassword-sentemail.html")
                .unwrap_or_else(|_| "<h1>Password reset email sent</h1>".to_string());
            let html = html.replace("{{ reguser }}", &form.email);
            Ok(Html(html))
        }
        Ok(None) => {
            let html = std::fs::read_to_string("web/templates/lostpassword-baduser.html")
                .unwrap_or_else(|_| "<h1>User not found</h1>".to_string());
            let html = html.replace("{{ reguser }}", &form.email);
            Ok(Html(html))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

async fn send_reset_email(state: &AppState, email: &str, token: &str) {
    let base = state
        .config
        .public_base_url
        .clone()
        .unwrap_or_else(|| format!("http://localhost:{}", state.config.port));
    let link = match reqwest::Url::parse(&format!("{}/pwreset", base.trim_end_matches('/'))) {
        Ok(mut url) => {
            url.query_pairs_mut()
                .append_pair("u", email)
                .append_pair("d", token);
            url.to_string()
        }
        Err(e) => {
            tracing::error!("Invalid PUBLIC_BASE_URL: {:?}", e);
            return;
        }
    };

    if state.config.environment == "development" {
        tracing::info!("Password reset link (development only): {}", link);
    }

    let body = format!(
        "<p>Use the link below to choose a new password. It can be used once and expires in one hour.</p><p><a href=\"{}\">Reset password</a></p>",
        html_escape(&link)
    );
    let service = EmailService::new(&state.config).await;
    if let Err(e) = service
        .send_html_email(email, "Reset your password", &body)
        .await
    {
        tracing::error!("Failed to send password reset email: {:?}", e);
    }
}

pub async fn password_reset(
    State(state): State<AppState>,
    Form(form): Form<PasswordResetRequest>,
) -> Result<Response, StatusCode> {
    match reset_password_with_token(
        &state.db,
        &form.email,
        &form.token,
        &form.password,
        reset_token::now(),
    )
    .await
    {
        Ok(ResetOutcome::Reset) => {
            let html = std::fs::read_to_string("web/templates/pwreset-ok.html")
                .unwrap_or_else(|_| "<h1>Password reset successful</h1>".to_string());
            let html = html.replace("{{ reguser }}", &html_escape(&form.email));
            Ok(Html(html).into_response())
        }
        Ok(ResetOutcome::Invalid) => {
            let html = std::fs::read_to_string("web/templates/pwreset-invalid.html")
                .unwrap_or_else(|_| "<h1>Invalid or expired reset link</h1>".to_string());
            let html = html.replace("{{ reguser }}", &html_escape(&form.email));
            Ok((StatusCode::BAD_REQUEST, Html(html)).into_response())
        }
        Err(e) => {
            tracing::error!("Password reset error: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum ResetOutcome {
    Reset,
    Invalid,
}

/// Changes the password only if `token` is the valid, unexpired, unused reset
/// token issued to this user. The token is consumed atomically.
pub async fn reset_password_with_token(
    db: &Database,
    email: &str,
    token: &str,
    new_password: &str,
    now: i64,
) -> anyhow::Result<ResetOutcome> {
    let Some(user) = db.get_user_by_email(email).await? else {
        return Ok(ResetOutcome::Invalid);
    };
    let Some(stored) = user.dongle.as_deref() else {
        return Ok(ResetOutcome::Invalid);
    };
    if new_password.is_empty() || !reset_token::matches(stored, token, now) {
        return Ok(ResetOutcome::Invalid);
    }
    let password_hash = hash_password(new_password)?;
    if db
        .reset_user_password_with_dongle(user.id, stored, &password_hash)
        .await?
    {
        Ok(ResetOutcome::Reset)
    } else {
        Ok(ResetOutcome::Invalid)
    }
}

// API endpoints for JSON responses
pub async fn api_login(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<LoginRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), StatusCode> {
    let password = payload
        .password
        .or(payload.pwd)
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Get user
    let user = match state.db.get_user_by_email(&payload.email).await {
        Ok(Some(user)) => user,
        Ok(None) => {
            return Ok((
                jar,
                Json(json!({
                    "result": "fail",
                    "data": "authfail",
                    "message": "Invalid credentials"
                })),
            ));
        }
        Err(e) => {
            tracing::error!("Database error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Verify password
    let password_valid = match verify_password(&password, &user.password_hash) {
        Ok(valid) => valid,
        Err(e) => {
            tracing::error!("Password verification error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    if !password_valid {
        return Ok((
            jar,
            Json(json!({
                "result": "fail",
                "data": "authfail",
                "message": "Invalid credentials"
            })),
        ));
    }

    let [c1, c2] = create_user_cookie(&payload.email, user.id);
    let jar = jar.add(c1).add(c2);

    Ok((
        jar,
        Json(json!({
            "result": "ok",
            "data": "success",
            "user": {
                "email": user.email,
                "id": user.id
            }
        })),
    ))
}

pub async fn api_register(
    State(state): State<AppState>,
    jar: CookieJar,
    Json(payload): Json<RegisterRequest>,
) -> Result<(CookieJar, Json<serde_json::Value>), StatusCode> {
    let password = payload
        .password
        .or(payload.pwd)
        .ok_or(StatusCode::BAD_REQUEST)?;

    // Validate email
    if !payload.email.contains('@') {
        return Ok((
            jar,
            Json(json!({
                "result": "fail",
                "data": "usererror",
                "message": "Invalid email format"
            })),
        ));
    }

    // Check if user exists
    match state.db.get_user_by_email(&payload.email).await {
        Ok(Some(_)) => {
            return Ok((
                jar,
                Json(json!({
                    "result": "fail",
                    "data": "userexists",
                    "message": "User already exists"
                })),
            ));
        }
        Ok(None) => {}
        Err(e) => {
            tracing::error!("Database error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    }

    // Hash password
    let password_hash = match hash_password(&password) {
        Ok(hash) => hash,
        Err(e) => {
            tracing::error!("Password hashing error: {:?}", e);
            return Err(StatusCode::INTERNAL_SERVER_ERROR);
        }
    };

    // Create user
    match state.db.create_user(&payload.email, &password_hash).await {
        Ok(user) => {
            tracing::info!("User created successfully: {}", payload.email);

            let [c1, c2] = create_user_cookie(&payload.email, user.id);
            let jar = jar.add(c1).add(c2);

            Ok((
                jar,
                Json(json!({
                    "result": "ok",
                    "data": "success",
                    "message": "Registration successful",
                    "user": {
                        "email": user.email,
                        "id": user.id
                    }
                })),
            ))
        }
        Err(e) => {
            tracing::error!("User creation error: {:?}", e);
            Err(StatusCode::INTERNAL_SERVER_ERROR)
        }
    }
}

pub async fn api_logout(jar: CookieJar) -> (CookieJar, Json<serde_json::Value>) {
    let [c1, c2] = clear_user_cookie();
    let jar = jar.add(c1).add(c2).remove(Cookie::from("session"));

    (
        jar,
        Json(json!({
            "result": "ok"
        })),
    )
}

pub async fn api_me(
    State(state): State<AppState>,
    jar: CookieJar,
) -> Result<Json<serde_json::Value>, StatusCode> {
    if let Some(user_email) = get_current_user(&jar) {
        if let Ok(Some(user)) = state.db.get_user_by_email(&user_email).await {
            return Ok(Json(json!({
                "email": user.email,
                "id": user.id
            })));
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}

#[cfg(test)]
mod tests {
    //! These need disposable databases, so they are ignored by default:
    //! `TEST_MYSQL_DSN=mysql://root:pw@127.0.0.1:3306/db cargo test -- --ignored`

    use super::*;
    use crate::auth::verify_password;
    use uuid::Uuid;

    const OLD_PW: &str = "old-password-1";
    const NEW_PW: &str = "new-password-2";

    async fn test_db() -> Database {
        let mongo = std::env::var("TEST_MONGO_URI")
            .unwrap_or_else(|_| "mongodb://127.0.0.1:27117".to_string());
        let mysql = std::env::var("TEST_MYSQL_DSN")
            .unwrap_or_else(|_| "mysql://root:throwaway@127.0.0.1:33317/touchcalc_test".into());
        Database::new(&mongo, "tc_unit_tests", &mysql)
            .await
            .unwrap()
    }

    async fn new_user(db: &Database) -> String {
        let email = format!("{}@example.test", Uuid::new_v4());
        db.create_user(&email, &hash_password(OLD_PW).unwrap())
            .await
            .unwrap();
        email
    }

    async fn issue(db: &Database, email: &str, issued_at: i64) -> String {
        let user = db.get_user_by_email(email).await.unwrap().unwrap();
        let (token, stored) = reset_token::generate(issued_at);
        db.set_user_dongle(user.id, &stored).await.unwrap();
        token
    }

    async fn password_is(db: &Database, email: &str, pw: &str) -> bool {
        let user = db.get_user_by_email(email).await.unwrap().unwrap();
        verify_password(pw, &user.password_hash).unwrap()
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn reset_without_a_token_does_not_change_the_password() {
        let db = test_db().await;
        let email = new_user(&db).await;
        let now = reset_token::now();

        let out = reset_password_with_token(&db, &email, "", NEW_PW, now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);

        issue(&db, &email, now).await;
        let out = reset_password_with_token(&db, &email, "", NEW_PW, now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);
        let out = reset_password_with_token(&db, &email, "guess", NEW_PW, now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);
        assert!(password_is(&db, &email, OLD_PW).await);
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn valid_token_resets_once_only() {
        let db = test_db().await;
        let email = new_user(&db).await;
        let now = reset_token::now();
        let token = issue(&db, &email, now).await;

        let out = reset_password_with_token(&db, &email, &token, NEW_PW, now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Reset);
        assert!(password_is(&db, &email, NEW_PW).await);
        assert!(!password_is(&db, &email, OLD_PW).await);

        let out = reset_password_with_token(&db, &email, &token, "third-pw-3", now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);
        assert!(password_is(&db, &email, NEW_PW).await);
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn expired_token_is_rejected() {
        let db = test_db().await;
        let email = new_user(&db).await;
        let now = reset_token::now();
        let token = issue(&db, &email, now - reset_token::RESET_TOKEN_TTL_SECS - 1).await;

        let out = reset_password_with_token(&db, &email, &token, NEW_PW, now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);
        assert!(password_is(&db, &email, OLD_PW).await);
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn token_is_tied_to_the_user_it_was_issued_to() {
        let db = test_db().await;
        let (alice, bob) = (new_user(&db).await, new_user(&db).await);
        let now = reset_token::now();
        let alice_token = issue(&db, &alice, now).await;
        issue(&db, &bob, now).await;

        let out = reset_password_with_token(&db, &bob, &alice_token, NEW_PW, now).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);
        assert!(password_is(&db, &bob, OLD_PW).await);
    }

    #[tokio::test]
    #[ignore = "requires MySQL"]
    async fn unknown_user_is_rejected() {
        let db = test_db().await;
        let out = reset_password_with_token(&db, "nobody@example.test", "t", NEW_PW, 0).await;
        assert_eq!(out.unwrap(), ResetOutcome::Invalid);
    }
}
