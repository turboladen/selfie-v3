// src/ports/config_loader.rs
use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::domain::config::FileConfig;

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
}

/// Port for loading configuration from disk
#[mockall::automock]
pub trait ConfigLoader {
    /// Load configuration from standard locations
    fn load_config(&self) -> Result<FileConfig, ConfigLoadError>;

    /// Load configuration from a specific path
    fn load_config_from_path(&self, path: &Path) -> Result<FileConfig, ConfigLoadError>;

    /// Find possible configuration file paths
    fn find_config_paths(&self) -> Vec<PathBuf>;

    /// Get the default configuration
    fn default_config(&self) -> FileConfig;
}
