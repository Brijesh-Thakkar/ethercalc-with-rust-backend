use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{Html, IntoResponse, Redirect},
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use serde::Deserialize;
use serde_json::json;

use crate::{handlers::get_current_user, AppState};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct DropboxQuery {
    action: Option<String>,
}

// Handle landing page
pub async fn handle_landing(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    let user = get_current_user(&jar, &state.session);

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

    // Try to read the file
    let content = std::fs::read_to_string(format!("web/templates/{}", slug))
        .unwrap_or_else(|_| String::new());

    Html(content)
}

// Handle Amazon web app routes
pub async fn handle_amazon_webapp(
    State(state): State<AppState>,
    Path((param1, param_code, param2)): Path<(String, String, String)>,
    jar: CookieJar,
) -> Result<impl IntoResponse, StatusCode> {
    // Check if user is logged in
    let user = get_current_user(&jar, &state.session);
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
        msc_data = std::fs::read_to_string(&msc_file).unwrap_or_else(|e| {
            tracing::warn!("Failed to read MSC file {}: {}", msc_file, e);
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
    let config_file = format!("{}/{}/{}.config.txt", msc_path, app_name, app_name);
    let config: serde_json::Value = std::fs::read_to_string(&config_file)
        .ok()
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

async fn handle_app_splash(app_name: String) -> Result<impl IntoResponse, StatusCode> {
    let msc_path = "webappTemplates";
    let splash_file = format!("{}/{}/appsplash.png", msc_path, app_name);

    match std::fs::read(&splash_file) {
        Ok(data) => Ok(([(axum::http::header::CONTENT_TYPE, "image/png")], data).into_response()),
        Err(_) => {
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
    let static_path = format!("web/static/runappios43c/{}", filename);

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
