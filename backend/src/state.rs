//! Application state shared across handlers via Axum extensions.

use std::sync::Arc;

use sqlx::PgPool;

use crate::config::Config;
use crate::github::GithubClient;
use crate::providers::ArchiveProvider;
use crate::queue::EventQueue;
use crate::storage::Storage;

/// Cloneable handle to all application services.
#[derive(Clone)]
pub struct AppState {
    pub config: Arc<Config>,
    pub pool: PgPool,
    pub github: GithubClient,
    pub storage: Arc<dyn Storage>,
    pub queue: EventQueue,
    /// External archive providers consulted when GitHub 404s.
    pub providers: Arc<Vec<Box<dyn ArchiveProvider>>>,
}

impl AppState {
    /// Build the application state from configuration.
    pub async fn from_config(config: Config) -> anyhow::Result<Self> {
        let pool = crate::db::connect(&config.database).await?;

        if config.database.auto_migrate {
            crate::db::run_migrations(&pool).await?;
        }

        let storage = crate::storage::from_config(&config.storage)?;
        let github = GithubClient::new(&config.github)?;
        let providers = crate::providers::build_providers(&config.providers.disabled);

        Ok(Self {
            config: Arc::new(config),
            pool,
            github,
            // Arc::from(Box<dyn Storage>) avoids a blanket impl on Box.
            storage: Arc::from(storage),
            queue: EventQueue::new(1_024),
            providers: Arc::new(providers),
        })
    }
}
