// src/ports/config_loader.rs
use std::path::PathBuf;

use thiserror::Error;

use crate::domain::config::AppConfig;

use super::application::ApplicationArguments;

#[derive(Error, Debug)]
pub enum ConfigLoadError {
    #[error("Failed to read configuration file: {0}")]
    ReadError(String),

    #[error("Failed to parse configuration file: {0}")]
    ParseError(String),

    #[error("No configuration file found in standard locations")]
    NotFound,

    #[error("Multiple configuration files found: {0}")]
    MultipleFound(String),

    #[error("Invalid configuration: {0}")]
    ValidationError(String),

    #[error(transparent)]
    ConfigError(#[from] config::ConfigError),
}

/// Port for loading configuration from disk
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait ConfigLoader: Send + Sync {
    /// Load configuration from standard locations
    fn load_config(&self, app_args: &ApplicationArguments) -> Result<AppConfig, ConfigLoadError>;

    /// Find possible configuration file paths
    fn find_config_paths(&self) -> Vec<PathBuf>;

    /// Get the default configuration
    fn default_config(&self) -> AppConfig;
}

#[cfg(test)]
impl MockConfigLoader {
    pub(crate) fn mock_load_config_ok(
        &mut self,
        app_args: ApplicationArguments,
        config: AppConfig,
    ) {
        let config = config.apply_cli_args(&app_args);

        self.expect_load_config()
            .with(mockall::predicate::eq(app_args))
            .returning(move |_| Ok(config.clone()));
    }

    pub(crate) fn mock_load_config_err(
        &mut self,
        app_args: ApplicationArguments,
        error: ConfigLoadError,
    ) {
        self.expect_load_config()
            .with(mockall::predicate::eq(app_args))
            .return_once(|_| Err(error));
    }
}
