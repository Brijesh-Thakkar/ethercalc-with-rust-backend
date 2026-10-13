use axum::{
    routing::{get, post},
    Router,
};
use tower_http::cors::CorsLayer;
use tower_http::services::ServeDir;
use tracing::info;
use tracing_subscriber::fmt::init;

mod auth;
mod config;
mod db;
mod handlers;
mod middleware;
mod models;
mod reset_token;
mod services;
mod session;
mod utils;

use config::AppConfig;
use db::Database;
use session::SessionManager;

#[derive(Clone)]
pub struct AppState {
    pub db: Database,
    pub config: AppConfig,
    pub session_manager: SessionManager,
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    init();

    let config = AppConfig::load().expect("Failed to load configuration");
    let db = Database::new(&config.mongo_uri, &config.mongo_database, &config.mysql_dsn).await?;
    let session_manager = SessionManager::new();

    let port = config.port;
    let static_path = config.static_path.clone();
    let state = AppState {
        db,
        config,
        session_manager,
    };

    let api_routes = Router::new()
        .route("/auth/register", post(handlers::auth::api_register))
        .route("/auth/login", post(handlers::auth::api_login))
        .route("/auth/logout", post(handlers::auth::api_logout))
        .route("/auth/me", get(handlers::auth::api_me))
        .with_state(state.clone());

    let protected_routes = Router::new()
        .route("/dashboard", get(handlers::app::handle_landing))
        .route(
            "/save",
            get(handlers::save::handle_save_get).post(handlers::save::handle_save_post),
        )
        .route("/usersheet", post(handlers::user_sheet::handle_user_sheet))
        .route(
            "/import",
            get(handlers::import::import_page).post(handlers::import::handle_import),
        )
        .route("/downloadfile", post(handlers::download::download_file))
        .route(
            "/htmltopdf",
            get(handlers::pdf::get_pdf).post(handlers::pdf::convert_html_to_pdf),
        )
        .route("/iwebapp", post(handlers::webapp::handle_webapp_post))
        .route("/irunasemailer", post(handlers::email::send_email))
        .route("/browser", get(handlers::app::handle_landing))
        .route(
            "/browser/:param1/:param_code/:param2",
            get(handlers::app::handle_amazon_webapp),
        )
        .route(
            "/browser/:param1/dropbox",
            get(handlers::dropbox::handle_dropbox).post(handlers::dropbox::handle_dropbox_post),
        )
        .route(
            "/browser/static/*filepath",
            get(handlers::app::handle_google_verification),
        )
        .layer(axum::middleware::from_fn(middleware::auth_guard))
        .with_state(state.clone());

    let app = Router::new()
        .route("/health", get(handlers::health_check))
        .route("/", get(handlers::home))
        .nest("/api", api_routes)
        .route("/iauth", post(handlers::auth::handle_iauth))
        .route(
            "/login",
            get(handlers::auth::login_page).post(handlers::auth::handle_login),
        )
        .route(
            "/register",
            get(handlers::auth::register_page).post(handlers::auth::handle_register),
        )
        .route(
            "/logout",
            get(handlers::auth::handle_logout).post(handlers::auth::handle_logout),
        )
        .route(
            "/pwreset",
            get(handlers::auth::password_reset_page).post(handlers::auth::password_reset),
        )
        .route(
            "/lostpw",
            get(handlers::auth::lost_password_page).post(handlers::auth::lost_password),
        )
        .merge(protected_routes)
        .nest_service("/static", ServeDir::new(&static_path))
        .nest_service("/js", ServeDir::new(format!("{}/js", &static_path)))
        .nest_service("/css", ServeDir::new(format!("{}/css", &static_path)))
        .nest_service("/images", ServeDir::new(format!("{}/images", &static_path)))
        .layer(axum::middleware::from_fn(middleware::logger))
        .layer(CorsLayer::permissive())
        .with_state(state);

    let addr = format!("0.0.0.0:{}", port);
    let listener = tokio::net::TcpListener::bind(&addr).await?;
    info!("Server starting on http://{}", addr);

    axum::serve(listener, app).await?;
    Ok(())
}
