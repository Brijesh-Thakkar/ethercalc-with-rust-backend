use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Deserialize;
use serde_json::json;

use crate::{handlers::get_current_user, safe_path, AppState};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct DropboxQuery {
    action: Option<String>,
}

// Handle landing page
pub async fn handle_landing(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    let user = get_current_user(&jar);

    tracing::info!("Landing page - current user: {:?}", user);

    let html = std::fs::read_to_string("web/templates/landing-page.html").unwrap_or_else(|_| {
        r#"<!DOCTYPE html>
<html>
<head><title>TouchCalc</title></head>
<body>
    <h1>Welcome to TouchCalc</h1>
    <p>User: {{ user }}</p>
    <p>Storage: {{ storage_backend }}</p>
</body>
</html>"#
            .to_string()
    });

    let user_display = user.unwrap_or_else(|| "Guest".to_string());
    let html = html.replace("{{ user }}", &user_display);
    let html = html.replace("{{ storage_backend }}", &state.config.storage_backend);
    let html = html.replace("{{ environment }}", &state.config.environment);
    let html = html.replace(
        "{{ debug }}",
        if state.config.environment == "development" {
            "true"
        } else {
            "false"
        },
    );

    Html(html)
}

// Handle Google verification files
pub async fn handle_google_verification(Path(filepath): Path<String>) -> impl IntoResponse {
    let slug = filepath.trim_start_matches('/');

    let path = match safe_path::resolve_under("web/templates", slug) {
        Ok(path) => path,
        Err(safe_path::PathError::Rejected) => return StatusCode::NOT_FOUND.into_response(),
        Err(safe_path::PathError::NotFound) => return Html(String::new()).into_response(),
    };

    // Try to read the file
    let content = std::fs::read_to_string(path).unwrap_or_default();

    Html(content).into_response()
}

// Handle Amazon web app routes
pub async fn handle_amazon_webapp(
    State(state): State<AppState>,
    Path((param1, param_code, param2)): Path<(String, String, String)>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    // Check if user is logged in
    let user = get_current_user(&jar);
    if user.is_none() {
        return Ok(Redirect::to("/browser").into_response());
    }

    let user = user.unwrap();

    // Get or create session
    let session_id = jar
        .get("session")
        .map(|c| c.value().to_string())
        .unwrap_or_else(|| uuid::Uuid::new_v4().to_string());

    if param2 == "index.html" {
        Ok(
            handle_webapp_index(state, param1, param_code, session_id, user, jar)
                .await?
                .into_response(),
        )
    } else if param2 == "appsplash.png" {
        Ok(handle_app_splash(param1).await?.into_response())
    } else {
        Ok(handle_static_file(param2).await?.into_response())
    }
}

async fn handle_webapp_index(
    state: AppState,
    app_name: String,
    param_code: String,
    session_id: String,
    user: String,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    let msc_path = "webappTemplates";

    if !safe_path::is_safe_relative(&app_name) {
        return Err(StatusCode::NOT_FOUND);
    }

    // Try to load existing spreadsheet data from storage first
    let mut msc_data = String::new();

    // Try to load from storage if user exists
    if !user.is_empty() {
        // Construct path for stored data
        let file_path = format!("home/{}/securestore/{}/{}.msc", user, app_name, app_name);

        // TODO: Implement actual storage retrieval when DB is ready
        // For now, just log the attempt
        tracing::debug!("Would attempt to load from storage: {}", file_path);
    }

    // If no stored data found, try file system
    if msc_data.is_empty() {
        let msc_file = format!("{}/{}/{}.msc.txt", msc_path, app_name, app_name);
        msc_data = read_confined_string(msc_path, &format!("{0}/{0}.msc.txt", app_name))
            .unwrap_or_else(|| {
                tracing::warn!("Failed to read MSC file {}", msc_file);
                // Proper SocialCalc save format as fallback
                format!(
                    r#"# SocialCalc Spreadsheet Control Save
version:1.5
part:sheet
cell:A1:v:Welcome to TouchCalc
cell:B1:v:Hello {}
cell:A2:v:Start editing here
cell:B2:v:Your data auto-saves
sheet:c:5:r:10:tvf:1
part:end
"#,
                    user
                )
            });
        tracing::info!(
            "Loaded MSC data from {}: {} bytes",
            msc_file,
            msc_data.len()
        );
    }

    // Read config file
    let config: serde_json::Value = read_confined_string(
        msc_path,
        &format!("{0}/{0}.config.txt", app_name),
    )
    .and_then(|data| serde_json::from_str(&data).ok())
    .unwrap_or_else(|| {
        json!({
            "code": param_code,
            "footers": ["Sheet1", "Sheet2", "Sheet3", "Sheet4", "Sheet5", "Sheet6", "Sheet7"]
        })
    });

    // Validate code (skip validation if no code specified)
    if let Some(code) = config.get("code").and_then(|c| c.as_str()) {
        if !code.is_empty() && code != param_code {
            return Ok(Html("Invalid access code".to_string()).into_response());
        }
    }

    // Get footers
    let footers = config
        .get("footers")
        .and_then(|f| f.as_array())
        .map(|arr| {
            arr.iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect::<Vec<_>>()
        })
        .unwrap_or_else(|| {
            vec![
                "Sheet1".to_string(),
                "Sheet2".to_string(),
                "Sheet3".to_string(),
                "Sheet4".to_string(),
                "Sheet5".to_string(),
                "Sheet6".to_string(),
                "Sheet7".to_string(),
            ]
        });

    // Update session
    let mut session = state.session_manager.get_or_create(&session_id);
    session.set_value("appName", json!(app_name));
    session.set_value("user", json!(user));
    state.session_manager.set(&session_id, session);

    // Check dropbox login status
    let db_login = if let Some(sess) = state.session_manager.get(&session_id) {
        sess.get_string("dbLogin").unwrap_or_default() == "1"
    } else {
        false
    };

    // Load template
    let html = std::fs::read_to_string("web/templates/amazonwebapp.html")
        .unwrap_or_else(|_| include_str!("../../web/templates/amazonwebapp.html").to_string());

    // Simple template replacement (Go template format uses {{.variable}})
    let html = html.replace("{{.fname}}", &app_name);

    // Properly escape the MSC data for JavaScript
    // Need to escape newlines, quotes, backticks, and backslashes for JS template literal
    let escaped_msc_data = msc_data
        .replace("\\", "\\\\") // Escape backslashes first
        .replace("\r", "\\r") // Escape carriage returns
        .replace("\n", "\\n") // Escape newlines
        .replace("`", "\\`") // Escape backticks
        .replace("${", "\\${") // Escape template literal expressions
        .replace("\"", "\\\""); // Escape double quotes

    let html = html.replace("{{.sheetstr}}", &escaped_msc_data);
    let html = html.replace("{{.sessionid}}", &session_id);
    let html = html.replace("{{.dbLogin}}", if db_login { "1" } else { "0" });
    let html = html.replace("{{.user}}", &user);
    let html = html.replace("{{.storage}}", &state.config.storage_backend);
    let sheets_json = serde_json::to_string(&footers).unwrap_or_default();
    let html = html.replace("{{.sheets}}", &sheets_json);

    let mut response_jar = jar;
    let session_cookie = Cookie::build(("session", session_id))
        .path("/")
        .http_only(true)
        .build();
    response_jar = response_jar.add(session_cookie);

    Ok((response_jar, Html(html)).into_response())
}

fn read_confined_string(root: &str, rel: &str) -> Option<String> {
    let path = safe_path::resolve_under(root, rel).ok()?;
    std::fs::read_to_string(path).ok()
}

async fn handle_app_splash(app_name: String) -> Result<impl IntoResponse, StatusCode> {
    let msc_path = "webappTemplates";
    let splash = safe_path::resolve_under(msc_path, &format!("{}/appsplash.png", app_name));
    if splash == Err(safe_path::PathError::Rejected) {
        return Err(StatusCode::NOT_FOUND);
    }

    match splash.ok().and_then(|p| std::fs::read(p).ok()) {
        Some(data) => Ok(([(axum::http::header::CONTENT_TYPE, "image/png")], data).into_response()),
        None => {
            // Return a default 1x1 PNG if splash doesn't exist
            let default_png = vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A];
            Ok((
                [(axum::http::header::CONTENT_TYPE, "image/png")],
                default_png,
            )
                .into_response())
        }
    }
}

async fn handle_static_file(filename: String) -> Result<impl IntoResponse, StatusCode> {
    let static_path = safe_path::resolve_under("web/static/runappios43c", &filename)
        .map_err(|_| StatusCode::NOT_FOUND)?;

    match std::fs::read(&static_path) {
        Ok(data) => {
            let content_type = mime_guess::from_path(&filename)
                .first_or_octet_stream()
                .to_string();
            Ok(([(axum::http::header::CONTENT_TYPE, content_type)], data).into_response())
        }
        Err(_) => Err(StatusCode::NOT_FOUND),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        http::{Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;

    async fn get_body(uri: &str) -> (StatusCode, String) {
        let app = Router::new().route("/browser/static/*filepath", get(handle_google_verification));
        let resp = app
            .oneshot(Request::builder().uri(uri).body(Body::empty()).unwrap())
            .await
            .unwrap();
        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), 1 << 20)
            .await
            .unwrap();
        (status, String::from_utf8_lossy(&body).into_owned())
    }

    #[tokio::test]
    async fn traversal_payloads_do_not_read_files_outside_the_template_dir() {
        let payloads = [
            "..%2f..%2fCargo.toml",
            "..%2F..%2FCargo.toml",
            "%2e%2e%2f%2e%2e%2fCargo.toml",
            "%2e%2e/%2e%2e/Cargo.toml",
            "../../Cargo.toml",
            "%252e%252e%252f%252e%252e%252fCargo.toml",
            "..%5c..%5cCargo.toml",
            "%2fetc%2fpasswd",
            "..%2f..%2f..%2f..%2f..%2f..%2fetc%2fpasswd",
        ];
        for p in payloads {
            let (status, body) = get_body(&format!("/browser/static/{p}")).await;
            assert!(
                !body.contains("aspiring-investments-backend") && !body.contains("root:"),
                "{p} leaked file contents"
            );
            assert_ne!(status, StatusCode::INTERNAL_SERVER_ERROR, "{p}");
        }
    }

    #[tokio::test]
    async fn files_inside_the_template_dir_are_still_served() {
        let (status, body) = get_body("/browser/static/base.html").await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            body,
            std::fs::read_to_string("web/templates/base.html").unwrap()
        );
    }
}
