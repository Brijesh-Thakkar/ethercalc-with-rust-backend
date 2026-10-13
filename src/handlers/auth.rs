use axum::{
    extract::{Query, State},
    http::StatusCode,
    response::{Html, IntoResponse, Json, Redirect, Response},
    Form,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use base64::{engine::general_purpose, Engine as _};
use rand::Rng;
use serde::Deserialize;
use serde_json::json;

use crate::{
    auth::{hash_password, verify_password},
    handlers::{clear_user_cookie, create_user_cookie, get_current_user},
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
<input type="password" name="password" placeholder="New Password" required />
<button type="submit">Reset Password</button>
</form></body></html>"#
            .to_string()
    });

    let html = html.replace("{{ reguser }}", &query.u);
    Html(html)
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

    let jar = jar.add(create_user_cookie(&state.session, &email, user.id));

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

            let jar = jar.add(create_user_cookie(&state.session, &email, user.id));

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

    let [c1, c2, c3] = clear_user_cookie();
    let jar = jar.add(c1).add(c2).add(c3).remove(Cookie::from("session"));

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
            // Generate dongle
            let dongle = generate_random_string(20);

            // Save dongle to user
            let _ = state.db.set_user_dongle(user.id, &dongle).await;

            // In real implementation, send email here
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

pub async fn password_reset(
    State(state): State<AppState>,
    Form(form): Form<PasswordResetRequest>,
) -> Result<Html<String>, StatusCode> {
    // Check if user exists
    match state.db.get_user_by_email(&form.email).await {
        Ok(Some(user)) => {
            // Hash new password
            let password_hash = match hash_password(&form.password) {
                Ok(hash) => hash,
                Err(_) => return Err(StatusCode::INTERNAL_SERVER_ERROR),
            };

            // Update password
            match state.db.update_user_password(user.id, &password_hash).await {
                Ok(_) => {
                    let html = std::fs::read_to_string("web/templates/pwreset-ok.html")
                        .unwrap_or_else(|_| "<h1>Password reset successful</h1>".to_string());
                    let html = html.replace("{{ reguser }}", &form.email);
                    Ok(Html(html))
                }
                Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
            }
        }
        Ok(None) => {
            let html = std::fs::read_to_string("web/templates/pwreset-invalid.html")
                .unwrap_or_else(|_| "<h1>Invalid reset link</h1>".to_string());
            let html = html.replace("{{ reguser }}", &form.email);
            Ok(Html(html))
        }
        Err(_) => Err(StatusCode::INTERNAL_SERVER_ERROR),
    }
}

fn generate_random_string(length: usize) -> String {
    let mut rng = rand::rng();
    let bytes: Vec<u8> = (0..length).map(|_| rng.random()).collect();
    general_purpose::URL_SAFE_NO_PAD.encode(&bytes)[..length].to_string()
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

    let jar = jar.add(create_user_cookie(&state.session, &payload.email, user.id));

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

            let jar = jar.add(create_user_cookie(&state.session, &payload.email, user.id));

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
    let [c1, c2, c3] = clear_user_cookie();
    let jar = jar.add(c1).add(c2).add(c3).remove(Cookie::from("session"));

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
    if let Some(user_email) = get_current_user(&jar, &state.session) {
        if let Ok(Some(user)) = state.db.get_user_by_email(&user_email).await {
            return Ok(Json(json!({
                "email": user.email,
                "id": user.id
            })));
        }
    }

    Err(StatusCode::UNAUTHORIZED)
}
