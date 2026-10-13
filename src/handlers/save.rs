use axum::{
    extract::State,
    http::{Method, Request, StatusCode},
    response::{Html, IntoResponse, Json, Redirect, Response},
    Form,
};
use axum_extra::extract::CookieJar;
use serde::{Deserialize, Serialize};
use serde_json::json;
use std::fs;

use crate::{handlers::get_current_user, AppState};

#[allow(dead_code)]
#[derive(Debug, Deserialize)]
pub struct SaveForm {
    pub fname: String,
    pub data: String,
}

#[derive(Debug, Serialize)]
struct FileEntry {
    fname: String,
}

// Combined handler for GET and POST /save
#[allow(dead_code)]
pub async fn handle_save<B>(
    State(state): State<AppState>,
    jar: CookieJar,
    req: Request<B>,
) -> Response {
    if req.method() == Method::GET {
        handle_save_get(State(state), jar).await.into_response()
    } else {
        // For POST, we need to extract the body
        // Since we can't consume the request here, return a simple response
        // The actual POST handler is handle_save_post
        (StatusCode::METHOD_NOT_ALLOWED, "Use POST handler").into_response()
    }
}

pub async fn handle_save_get(State(state): State<AppState>, jar: CookieJar) -> impl IntoResponse {
    let user = match get_current_user(&jar, &state.session) {
        Some(u) => u,
        None => return Redirect::to("/login").into_response(),
    };

    println!("DEBUG: Loading file list for user: {}", user);

    // TODO: Get user's files from storage
    // For now, return a default file list
    let entries = vec![FileEntry {
        fname: "default".to_string(),
    }];

    println!("DEBUG: Found {} files for user {}", entries.len(), user);

    // Load template
    let template_path = "web/templates/allusersheets.html";
    let template_content = match fs::read_to_string(template_path) {
        Ok(content) => content,
        Err(_) => {
            // Return simple HTML if template doesn't exist
            let mut html = String::from("<html><body><h2>Your Files</h2><ul>");
            for entry in entries {
                html.push_str(&format!("<li>{}</li>", entry.fname));
            }
            html.push_str("</ul></body></html>");
            return Html(html).into_response();
        }
    };

    // Simple template replacement
    let entries_json = serde_json::to_string(&entries).unwrap_or_else(|_| "[]".to_string());
    let html = template_content
        .replace("{{ entries }}", &entries_json)
        .replace("{{ user }}", &user);

    Html(html).into_response()
}

pub async fn handle_save_post(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(form): Form<SaveForm>,
) -> impl IntoResponse {
    let user = match get_current_user(&jar, &state.session) {
        Some(u) => u,
        None => {
            return Json(json!({
                "result": "fail",
                "data": "usererror"
            }))
            .into_response()
        }
    };

    println!("DEBUG: Saving file {} for user {}", form.fname, user);

    // TODO: Implement actual file saving to storage
    // For now, just return success
    Json(json!({
        "result": "ok",
        "data": "saved"
    }))
    .into_response()
}
