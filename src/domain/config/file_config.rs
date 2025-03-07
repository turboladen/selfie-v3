use std::path::{Path, PathBuf};

use serde::Deserialize;

use crate::domain::package::{EnvironmentConfig, Package, PackageValidationError};

use super::{validate_config::ValidateConfig, ConfigValidationError, LogConfig};

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FileConfig {
    pub environment: String,
    pub package_directory: PathBuf,

    #[serde(default)]
    pub command_timeout: Option<u64>,

    #[serde(default)]
    pub stop_on_error: Option<bool>,

    #[serde(default)]
    pub max_parallel_installations: Option<usize>,

    #[serde(default)]
    pub logging: Option<LogConfig>,
}

impl FileConfig {
    pub fn new(environment: String, package_directory: PathBuf) -> Self {
        Self {
            environment,
            package_directory,
            command_timeout: None,
            stop_on_error: None,
            max_parallel_installations: None,
            logging: None,
        }
    }

    /// Full validation for commands that require a complete configuration
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        Self::validate_environment(&self.environment)?;
        Self::validate_package_directory(&self.package_directory)?;

        // Validate optional parameters
        if let Some(timeout) = self.command_timeout {
            Self::validate_command_timeout(timeout)?;
        }

        if let Some(max_parallel) = self.max_parallel_installations {
            Self::validate_max_parallel_installations(max_parallel)?;
        }

        // Validate logging config if present
        if let Some(log_config) = &self.logging {
            if log_config.enabled {
                Self::validate_log_directory(log_config.directory.as_os_str())?;
                Self::validate_log_max_files(log_config.max_files)?;
                Self::validate_log_max_size(log_config.max_size)?;
            }
        }

        Ok(())
    }

    /// Minimal validation for commands that only require a package directory
    pub fn validate_minimal(&self) -> Result<(), ConfigValidationError> {
        if self.package_directory.as_os_str().is_empty() {
            return Err(ConfigValidationError::EmptyField(
                "package_directory".to_string(),
            ));
        }

        // Expand the package directory path
        let package_dir = self.package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        let expanded_path = Path::new(expanded_path.as_ref());

        if !expanded_path.is_absolute() {
            return Err(ConfigValidationError::InvalidPackageDirectory(
                "Package directory must be an absolute path".to_string(),
            ));
        }

        Ok(())
    }

    pub fn expanded_package_directory(&self) -> PathBuf {
        let package_dir = self.package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        PathBuf::from(expanded_path.as_ref())
    }

    pub fn resolve_environment<'a>(
        &self,
        package: &'a Package,
    ) -> Result<&'a EnvironmentConfig, ConfigValidationError> {
        if self.environment.is_empty() {
            return Err(ConfigValidationError::InvalidPackage(
                "Package has no environments".to_string(),
            ));
        }

        package
            .resolve_environment(&self.environment)
            .map_err(|e| match e {
                PackageValidationError::EnvironmentNotSupported(_) => {
                    ConfigValidationError::EnvironmentNotFound(self.environment.clone())
                }
                PackageValidationError::MissingField(_) => {
                    ConfigValidationError::InvalidPackage("Package has no environments".to_string())
                }
                _ => ConfigValidationError::InvalidPackage(
                    "Invalid package configuration".to_string(),
                ),
            })
    }
}

/// Builder pattern for testing
#[derive(Default)]
pub struct FileConfigBuilder {
    environment: String,
    package_directory: PathBuf,
    command_timeout: Option<u64>,
    stop_on_error: Option<bool>,
    max_parallel_installations: Option<usize>,
    logging: Option<LogConfig>,
}

impl FileConfigBuilder {
    pub fn environment(mut self, environment: &str) -> Self {
        self.environment = environment.to_string();
        self
    }

    pub fn package_directory(mut self, package_directory: &str) -> Self {
        self.package_directory = PathBuf::from(package_directory);
        self
    }

    pub fn command_timeout(mut self, timeout: u64) -> Self {
        self.command_timeout = Some(timeout);
        self
    }

    pub fn stop_on_error(mut self, stop: bool) -> Self {
        self.stop_on_error = Some(stop);
        self
    }

    pub fn max_parallel(mut self, max: usize) -> Self {
        self.max_parallel_installations = Some(max);
        self
    }

    pub fn build(self) -> FileConfig {
        FileConfig {
            environment: self.environment,
            package_directory: self.package_directory,
            command_timeout: self.command_timeout,
            stop_on_error: self.stop_on_error,
            max_parallel_installations: self.max_parallel_installations,
            logging: self.logging,
        }
    }
}
