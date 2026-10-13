use anyhow::Result;
use chrono::Utc;
use sqlx::{MySql, Pool, Row};
use uuid::Uuid;

use crate::models::User;

pub struct UserRepository<'a> {
    pool: Option<&'a Pool<MySql>>,
}

impl<'a> UserRepository<'a> {
    pub fn new(pool: Option<&'a Pool<MySql>>) -> Self {
        Self { pool }
    }

    fn get_pool(&self) -> Result<&Pool<MySql>> {
        self.pool
            .ok_or_else(|| anyhow::anyhow!("MySQL not available"))
    }

    pub async fn create(&self, email: &str, password_hash: &str) -> Result<User> {
        let pool = self.get_pool()?;

        let id = Uuid::new_v4();
        let now = Utc::now();

        sqlx::query(
            "INSERT INTO users (id, email, password_hash, created_at, updated_at) VALUES (?, ?, ?, ?, ?)"
        )
        .bind(id.to_string())
        .bind(email)
        .bind(password_hash)
        .bind(now)
        .bind(now)
        .execute(pool)
        .await?;

        Ok(User {
            id,
            email: email.to_string(),
            password_hash: password_hash.to_string(),
            dongle: None,
            created_at: now,
            updated_at: now,
        })
    }

    pub async fn get_by_email(&self, email: &str) -> Result<Option<User>> {
        let pool = self.get_pool()?;

        let row = sqlx::query(
            "SELECT id, email, password_hash, dongle, created_at, updated_at FROM users WHERE email = ?"
        )
        .bind(email)
        .fetch_optional(pool)
        .await?;

        Ok(row.map(|r| User {
            id: Uuid::parse_str(r.get("id")).unwrap(),
            email: r.get("email"),
            password_hash: r.get("password_hash"),
            dongle: r.get("dongle"),
            created_at: r.get("created_at"),
            updated_at: r.get("updated_at"),
        }))
    }

    pub async fn update_password(&self, user_id: Uuid, password_hash: &str) -> Result<()> {
        let pool = self.get_pool()?;

        sqlx::query("UPDATE users SET password_hash = ?, updated_at = ? WHERE id = ?")
            .bind(password_hash)
            .bind(Utc::now())
            .bind(user_id.to_string())
            .execute(pool)
            .await?;

        Ok(())
    }

    pub async fn reset_password_with_dongle(
        &self,
        user_id: Uuid,
        expected_dongle: &str,
        password_hash: &str,
    ) -> Result<bool> {
        let pool = self.get_pool()?;

        let result = sqlx::query(
            "UPDATE users SET password_hash = ?, dongle = NULL, updated_at = ? WHERE id = ? AND dongle = ?",
        )
        .bind(password_hash)
        .bind(Utc::now())
        .bind(user_id.to_string())
        .bind(expected_dongle)
        .execute(pool)
        .await?;

        Ok(result.rows_affected() == 1)
    }

    pub async fn set_dongle(&self, user_id: Uuid, dongle: &str) -> Result<()> {
        let pool = self.get_pool()?;

        sqlx::query("UPDATE users SET dongle = ?, updated_at = ? WHERE id = ?")
            .bind(dongle)
            .bind(Utc::now())
            .bind(user_id.to_string())
            .execute(pool)
            .await?;

        Ok(())
    }
}
