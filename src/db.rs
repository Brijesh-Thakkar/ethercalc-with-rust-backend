use crate::models::{FileData, InAppPurchase, User};
use anyhow::Result;
use mongodb::{bson::Document, Client as MongoClient, Collection, Database as MongoDatabase};
use sqlx::{mysql::MySqlPoolOptions, MySql, Pool};
use std::time::Duration;
use uuid::Uuid;

mod file_repository;
mod purchase_repository;
mod user_repository;

pub use file_repository::FileRepository;
pub use purchase_repository::PurchaseRepository;
pub use user_repository::UserRepository;

#[derive(Clone)]
pub struct Database {
    mongo_client: MongoClient,
    mongo_db: MongoDatabase,
    mysql_pool: Option<Pool<MySql>>,
}

impl Database {
    pub async fn new(mongo_uri: &str, mongo_db_name: &str, mysql_dsn: &str) -> Result<Self> {
        // Connect to MongoDB
        let mongo_client = MongoClient::with_uri_str(mongo_uri).await?;
        let mongo_db = mongo_client.database(mongo_db_name);

        // Connect to MySQL
        let mysql_pool = match Self::connect_mysql(mysql_dsn).await {
            Ok(pool) => {
                tracing::info!("MySQL connection established successfully");
                Some(pool)
            }
            Err(e) => {
                tracing::warn!("MySQL connection failed: {}. Continuing without MySQL.", e);
                None
            }
        };

        Ok(Self {
            mongo_client,
            mongo_db,
            mysql_pool,
        })
    }

    async fn connect_mysql(mysql_dsn: &str) -> Result<Pool<MySql>> {
        let mysql_pool = MySqlPoolOptions::new()
            .max_connections(5)
            .acquire_timeout(Duration::from_secs(30))
            .connect(mysql_dsn)
            .await?;

        Self::init_mysql_tables(&mysql_pool).await?;
        Ok(mysql_pool)
    }

    async fn init_mysql_tables(pool: &Pool<MySql>) -> Result<()> {
        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS users (
                id CHAR(36) PRIMARY KEY,
                email VARCHAR(255) UNIQUE NOT NULL,
                password_hash VARCHAR(255) NOT NULL,
                dongle VARCHAR(255),
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP
            )
            "#,
        )
        .execute(pool)
        .await?;

        sqlx::query(
            r#"
            CREATE TABLE IF NOT EXISTS in_app_purchases (
                id CHAR(36) PRIMARY KEY,
                user_id CHAR(36) NOT NULL,
                app_name VARCHAR(255) NOT NULL,
                owned INT NOT NULL DEFAULT 0,
                consumed INT NOT NULL DEFAULT 0,
                created_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP,
                updated_at TIMESTAMP DEFAULT CURRENT_TIMESTAMP ON UPDATE CURRENT_TIMESTAMP,
                FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE
            )
            "#,
        )
        .execute(pool)
        .await?;

        Ok(())
    }

    pub fn mongo_db(&self) -> &MongoDatabase {
        &self.mongo_db
    }

    pub fn mysql_pool(&self) -> Option<&Pool<MySql>> {
        self.mysql_pool.as_ref()
    }

    #[allow(dead_code)]
    pub fn mongo_client(&self) -> &MongoClient {
        &self.mongo_client
    }

    #[allow(dead_code)] // Used by FileRepository internally
    fn files_collection(&self) -> Collection<Document> {
        self.mongo_db.collection("files")
    }

    pub fn users(&self) -> UserRepository<'_> {
        UserRepository::new(self.mysql_pool.as_ref())
    }

    pub fn files(&self) -> FileRepository<'_> {
        FileRepository::new(&self.mongo_db)
    }

    pub fn purchases(&self) -> PurchaseRepository<'_> {
        PurchaseRepository::new(self.mysql_pool.as_ref())
    }

    pub async fn create_user(&self, email: &str, password_hash: &str) -> Result<User> {
        self.users().create(email, password_hash).await
    }

    pub async fn get_user_by_email(&self, email: &str) -> Result<Option<User>> {
        self.users().get_by_email(email).await
    }

    pub async fn update_user_password(&self, user_id: Uuid, password_hash: &str) -> Result<()> {
        self.users().update_password(user_id, password_hash).await
    }

    /// Sets the new password and clears the reset token, only if the stored
    /// token still equals `expected_dongle`. Returns whether it was consumed.
    pub async fn reset_user_password_with_dongle(
        &self,
        user_id: Uuid,
        expected_dongle: &str,
        password_hash: &str,
    ) -> Result<bool> {
        self.users()
            .reset_password_with_dongle(user_id, expected_dongle, password_hash)
            .await
    }

    pub async fn set_user_dongle(&self, user_id: Uuid, dongle: &str) -> Result<()> {
        self.users().set_dongle(user_id, dongle).await
    }

    pub async fn create_file(&self, user_id: Uuid, path: &str, content: &str) -> Result<FileData> {
        self.files().create(user_id, path, content).await
    }

    pub async fn get_file(&self, user_id: Uuid, path: &str) -> Result<Option<FileData>> {
        self.files().get(user_id, path).await
    }

    pub async fn update_file(&self, user_id: Uuid, path: &str, content: &str) -> Result<()> {
        self.files().update(user_id, path, content).await
    }

    pub async fn delete_file(&self, user_id: Uuid, path: &str) -> Result<()> {
        self.files().delete(user_id, path).await
    }

    pub async fn list_files(&self, user_id: Uuid, path_prefix: &str) -> Result<Vec<FileData>> {
        self.files().list(user_id, path_prefix).await
    }

    pub async fn get_or_create_purchase(
        &self,
        user_id: Uuid,
        app_name: &str,
    ) -> Result<InAppPurchase> {
        self.purchases().get_or_create(user_id, app_name).await
    }

    pub async fn update_purchase(
        &self,
        user_id: Uuid,
        app_name: &str,
        owned: i32,
        consumed: i32,
    ) -> Result<()> {
        self.purchases()
            .update(user_id, app_name, owned, consumed)
            .await
    }
}
