use axum::{
    extract::{Request, State},
    middleware::Next,
    response::{IntoResponse, Redirect, Response},
};
use axum_extra::extract::cookie::CookieJar;
use tower_http::cors::CorsLayer;

use crate::handlers::get_current_user_id;
use crate::session_token::SessionSigner;

#[allow(dead_code)]
pub fn cors() -> CorsLayer {
    CorsLayer::permissive()
}

pub async fn logger(request: Request, next: Next) -> Response {
    let method = request.method().clone();
    let uri = request.uri().clone();
    tracing::info!("{} {}", method, uri);
    let response = next.run(request).await;
    tracing::info!("{} {} - {}", method, uri, response.status());
    response
}

pub async fn auth_guard(
    State(signer): State<SessionSigner>,
    jar: CookieJar,
    mut request: Request,
    next: Next,
) -> Response {
    let user_id = match get_current_user_id(&jar, &signer) {
        Some(id) => id,
        None => return Redirect::to("/login").into_response(),
    };
    request.extensions_mut().insert(user_id);
    next.run(request).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::{
        body::Body,
        extract::Extension,
        http::{header, Request, StatusCode},
        routing::get,
        Router,
    };
    use tower::ServiceExt;
    use uuid::Uuid;

    use crate::session_token::{unix_now, SESSION_COOKIE};

    fn signer() -> SessionSigner {
        SessionSigner::new(&[7u8; 32], false).unwrap()
    }

    fn app() -> Router {
        Router::new()
            .route(
                "/protected",
                get(|Extension(id): Extension<Uuid>| async move { id.to_string() }),
            )
            .layer(axum::middleware::from_fn_with_state(signer(), auth_guard))
    }

    async fn status_with_cookie(cookie: Option<String>) -> StatusCode {
        let mut req = Request::builder().uri("/protected");
        if let Some(c) = cookie {
            req = req.header(header::COOKIE, c);
        }
        app()
            .oneshot(req.body(Body::empty()).unwrap())
            .await
            .unwrap()
            .status()
    }

    #[tokio::test]
    async fn forged_user_id_cookie_is_rejected() {
        let victim = Uuid::new_v4();
        let status = status_with_cookie(Some(format!("user_id={victim}"))).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn forged_user_id_and_user_cookies_are_rejected() {
        let victim = Uuid::new_v4();
        let status = status_with_cookie(Some(format!(
            "user_id={victim}; user=\"victim@example.com\""
        )))
        .await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn bare_uuid_in_session_cookie_is_rejected() {
        let status = status_with_cookie(Some(format!("{SESSION_COOKIE}={}", Uuid::new_v4()))).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn no_cookie_is_rejected() {
        assert_eq!(status_with_cookie(None).await, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn token_signed_with_another_key_is_rejected() {
        let other = SessionSigner::new(&[9u8; 32], false).unwrap();
        let token = other.issue(Uuid::new_v4(), "a@b.c", unix_now());
        let status = status_with_cookie(Some(format!("{SESSION_COOKIE}={token}"))).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn expired_token_is_rejected() {
        let token = signer().issue(Uuid::new_v4(), "a@b.c", 0);
        let status = status_with_cookie(Some(format!("{SESSION_COOKIE}={token}"))).await;
        assert_eq!(status, StatusCode::SEE_OTHER);
    }

    #[tokio::test]
    async fn valid_signed_session_is_accepted() {
        let id = Uuid::new_v4();
        let token = signer().issue(id, "a@b.c", unix_now());
        let req = Request::builder()
            .uri("/protected")
            .header(header::COOKIE, format!("{SESSION_COOKIE}={token}"))
            .body(Body::empty())
            .unwrap();
        let resp = app().oneshot(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), 1024).await.unwrap();
        assert_eq!(body, id.to_string().as_bytes());
    }
}
