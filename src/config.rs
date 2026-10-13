use config::{Config, ConfigError, Environment};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppConfig {
    #[serde(default = "default_environment")]
    pub environment: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub jwt_secret: String,
    pub cookie_secret: String,
    /// Public base URL used in emailed links (`PUBLIC_BASE_URL`).
    #[serde(default)]
    pub public_base_url: Option<String>,

    #[serde(default = "default_storage_backend")]
    pub storage_backend: String,

    #[serde(default = "default_mongo_uri")]
    pub mongo_uri: String,
    #[serde(default = "default_mongo_database")]
    pub mongo_database: String,

    #[serde(default = "default_mysql_dsn")]
    pub mysql_dsn: String,

    #[serde(default = "default_minio_endpoint")]
    pub minio_endpoint: String,
    #[serde(default = "default_minio_access_key")]
    pub minio_access_key: String,
    #[serde(default = "default_minio_secret_key")]
    pub minio_secret_key: String,
    #[serde(default = "default_minio_bucket")]
    pub minio_bucket: String,
    #[serde(default)]
    pub minio_ssl: bool,

    #[serde(default)]
    pub aws_access_key_id: String,
    #[serde(default)]
    pub aws_secret_access_key: String,
    #[serde(default = "default_aws_region")]
    pub aws_region: String,
    #[serde(default = "default_s3_bucket")]
    pub s3_bucket: String,

    #[serde(default = "default_from_email")]
    pub from_email: String,

    #[serde(default = "default_templates_path")]
    pub templates_path: String,
    #[serde(default = "default_static_path")]
    pub static_path: String,
}

fn default_environment() -> String {
    "development".to_string()
}
fn default_port() -> u16 {
    8080
}
fn default_storage_backend() -> String {
    "minio".to_string()
}
fn default_mongo_uri() -> String {
    "mongodb://localhost:27017".to_string()
}
fn default_mongo_database() -> String {
    "touchcalc".to_string()
}
fn default_mysql_dsn() -> String {
    "mysql://root:password@localhost:3306/touchcalc".to_string()
}
fn default_minio_endpoint() -> String {
    "localhost:9000".to_string()
}
fn default_minio_access_key() -> String {
    "minioadmin".to_string()
}
fn default_minio_secret_key() -> String {
    "minioadmin".to_string()
}
fn default_minio_bucket() -> String {
    "touchcalc-storage".to_string()
}
fn default_aws_region() -> String {
    "us-east-1".to_string()
}
fn default_s3_bucket() -> String {
    "aspiring-cloud-storage".to_string()
}
fn default_from_email() -> String {
    "aspiring.investments@gmail.com".to_string()
}
fn default_templates_path() -> String {
    "./web/templates".to_string()
}
fn default_static_path() -> String {
    "./web/static".to_string()
}

impl AppConfig {
    pub fn load() -> Result<Self, ConfigError> {
        let _ = dotenvy::dotenv();

        let config = Config::builder()
            .set_default("jwt_secret", "change-me-in-production")?
            .set_default("cookie_secret", "change-me-in-production")?
            .add_source(Environment::default().try_parsing(true))
            .build()?;

        config.try_deserialize()
    }

    #[deprecated(since = "0.2.0", note = "Use AppConfig::load() instead")]
    pub fn from_env() -> anyhow::Result<Self> {
        Self::load().map_err(|e| anyhow::anyhow!("Failed to load config: {}", e))
    }
}
