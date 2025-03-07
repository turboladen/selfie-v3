// src/domain/config.rs
mod app_config;
mod file_config;
mod log_config;
mod validate_config;

use thiserror::Error;

pub use self::{
    app_config::{AppConfig, AppConfigBuilder},
    file_config::{FileConfig, FileConfigBuilder},
    log_config::LogConfig,
};

const COMMAND_TIMEOUT_DEFAULT: u64 = 60;
const VERBOSE_DEFAULT: bool = false;
const USE_COLORS_DEFAULT: bool = true;
const USE_UNICODE_DEFAULT: bool = true;
const STOP_ON_ERROR_DEFAULT: bool = true;
const LOGGING_ENABLED_DEFAULT: bool = false;
const LOG_MAX_FILES_DEFAULT: usize = 10;
const LOG_MAX_SIZE_DEFAULT: usize = 10;

#[derive(Error, Debug, PartialEq)]
pub enum ConfigValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Empty field: {0}")]
    EmptyField(String),

    #[error("Invalid package directory: {0}")]
    InvalidPackageDirectory(String),

    #[error("Environment not found: {0}")]
    EnvironmentNotFound(String),

    #[error("Invalid package: {0}")]
    InvalidPackage(String),

    #[error("Invalid command timeout: {0}")]
    InvalidCommandTimeout(String),

    #[error("Invalid max parallel setting: {0}")]
    InvalidMaxParallel(String),

    #[error("Invalid log configuration: {0}")]
    InvalidLogConfig(String),
}
