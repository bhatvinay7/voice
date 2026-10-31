pub mod listings;
pub mod users;
pub mod bookings;
pub mod contracts;
pub mod calls;
#[cfg(test)]
pub mod seed_data;
#[cfg(test)]
pub mod seed_knowledge;
mod postgres;

use crate::models::*;
use config::AppConfig;
use redis::Client;
use sqlx::{postgres::PgPoolOptions, PgPool};
use std::time::Duration;

pub type DbResult<T> = Result<T, String>;

#[derive(Clone)]
pub struct DatabaseStore {
    pub(crate) pool: PgPool,
    pub(crate) redis_client: Option<Client>,
    pub bucket_name: String,
}

impl DatabaseStore {
    /// Configure the pool without opening sockets. Services call initialize before serving.
    pub fn new() -> Self {
        let cfg = AppConfig::load().expect("Database configuration is required");
        Self::from_config(&cfg).expect("Invalid PostgreSQL configuration")
    }

    pub fn from_config(cfg: &AppConfig) -> DbResult<Self> {
        if cfg.postgres_url.trim().is_empty() {
            return Err("POSTGRES_URL or DATABASE_URL must be configured".into());
        }
        let pool = PgPoolOptions::new().max_connections(10)
            .acquire_timeout(Duration::from_secs(15))
            .connect_lazy(&cfg.postgres_url).map_err(|_| "Invalid PostgreSQL connection URL")?;
        let redis_client = Client::open(cfg.redis_url.clone()).ok();
        Ok(Self { pool, redis_client, bucket_name: cfg.r2_bucket_name.clone() })
    }

    /// Fail startup when Neon is unavailable or schema migrations have not been applied.
    pub async fn initialize(&self) -> DbResult<()> {
        sqlx::query("SELECT id FROM public.user_profiles LIMIT 0").execute(&self.pool).await.map_err(postgres::error)?;
        sqlx::query("SELECT redis_entry_id FROM public.stream_events LIMIT 0").execute(&self.pool).await.map_err(postgres::error)?;
        Ok(())
    }

    pub async fn get_knowledge_documents(&self) -> DbResult<Vec<KnowledgeDocument>> {
        self.all::<KnowledgeDocument>().await
    }
    pub async fn add_knowledge_document(&self, doc: KnowledgeDocument) -> DbResult<()> {
        self.save(&doc).await
    }
}

#[cfg(test)]
mod tests;
mod legacy;

mod conversation;
