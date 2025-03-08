// src/domain/config.rs
mod app_config;
mod file_config;
mod log_config;

use std::{
    borrow::Cow,
    fmt,
    num::{NonZeroU64, NonZeroUsize},
    path::Path,
};

use thiserror::Error;

pub use self::{
    app_config::{AppConfig, AppConfigBuilder},
    file_config::{FileConfig, FileConfigBuilder},
    log_config::LogConfig,
};

const COMMAND_TIMEOUT_DEFAULT: NonZeroU64 = unsafe { NonZeroU64::new_unchecked(60) };
const VERBOSE_DEFAULT: bool = false;
const USE_COLORS_DEFAULT: bool = true;
const USE_UNICODE_DEFAULT: bool = true;
const STOP_ON_ERROR_DEFAULT: bool = true;
const LOGGING_ENABLED_DEFAULT: bool = false;
const LOG_MAX_FILES_DEFAULT: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(10) };
const LOG_MAX_SIZE_DEFAULT: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(10) };
const MAX_PARALLEL_DEFAULT: NonZeroUsize = unsafe { NonZeroUsize::new_unchecked(4) };

#[derive(Error, Debug, PartialEq)]
pub struct ConfigValidationErrors(Vec<ConfigValidationError>);

impl fmt::Display for ConfigValidationErrors {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let strings: Vec<_> = self.0.iter().map(|e| e.to_string()).collect();
        write!(f, "{}", strings.join("\n"))
    }
}

impl ConfigValidationErrors {
    pub fn iter(&self) -> ConfigValidationErrorsIter {
        ConfigValidationErrorsIter {
            errors: &self.0,
            position: 0,
        }
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

pub struct ConfigValidationErrorsIter<'a> {
    errors: &'a [ConfigValidationError],
    position: usize,
}

impl<'a> Iterator for ConfigValidationErrorsIter<'a> {
    type Item = &'a ConfigValidationError;

    fn next(&mut self) -> Option<Self::Item> {
        if self.position < self.errors.len() {
            let error = &self.errors[self.position];
            self.position += 1;
            Some(error)
        } else {
            None
        }
    }
}

#[derive(Error, Debug, PartialEq)]
pub enum ConfigValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("{field} cannot be empty")]
    EmptyField { field: String },

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

    #[error("Invalid path: {field}")]
    InvalidPath { field: String, path: String },
}

/// Converting validation errors to our own error type
impl From<validator::ValidationErrors> for ConfigValidationErrors {
    fn from(validator_errors: validator::ValidationErrors) -> Self {
        let mut new_errors = Vec::new();

        for (field, field_errors) in validator_errors.field_errors() {
            for error in field_errors {
                let message = if let Some(msg) = &error.message {
                    format!("{}: {}", field, msg)
                } else {
                    format!("{}: Invalid value", field)
                };

                match &*error.code {
                    "InvalidPackageDirectory" => {
                        new_errors.push(ConfigValidationError::InvalidPackageDirectory(message));
                    }
                    "length" => new_errors.push(ConfigValidationError::EmptyField {
                        field: field.to_string(),
                    }),
                    "path" => new_errors.push(ConfigValidationError::InvalidPath {
                        field: field.to_string(),
                        path: error.params["value"].to_string(),
                    }),
                    _ => {
                        new_errors.push(ConfigValidationError::MissingField(message));
                    }
                }
            }
        }

        Self(new_errors)
    }
}

/// Custom validator for paths
fn validate_path(path: &Path) -> Result<(), validator::ValidationError> {
    if path.as_os_str().is_empty() {
        return Err(validator::ValidationError::new("length")
            .with_message(Cow::Borrowed("Path cannot be empty")));
    }

    // Expand the path to check if it's absolute
    let path_str = path.to_string_lossy();
    let expanded_path = shellexpand::tilde(&path_str);
    let expanded = std::path::Path::new(expanded_path.as_ref());

    if !expanded.is_absolute() {
        return Err(validator::ValidationError::new("path")
            .with_message(Cow::Borrowed("Path must be absolute")));
    }

    Ok(())
}

fn num_cpus_default() -> NonZeroUsize {
    NonZeroUsize::new(num_cpus::get()).unwrap_or(MAX_PARALLEL_DEFAULT)
}
