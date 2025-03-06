// src/domain/config.rs

use std::{
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use thiserror::Error;

use crate::domain::package::{EnvironmentConfig, Package, PackageValidationError};

const COMMAND_TIMEOUT_DEFAULT: u64 = 60;
const VERBOSE_DEFAULT: bool = false;
const USE_COLORS_DEFAULT: bool = true;
const USE_UNICODE_DEFAULT: bool = true;
const STOP_ON_ERROR_DEFAULT: bool = true;
const LOGGING_ENABLED_DEFAULT: bool = false;
const LOG_MAX_FILES_DEFAULT: usize = 10;
const LOG_MAX_SIZE_DEFAULT: usize = 10;

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct FileConfig {
    pub environment: String,
    pub package_directory: PathBuf,

    #[serde(default)]
    pub command_timeout: Option<u64>,

    #[serde(default)]
    pub stop_on_error: Option<bool>,

    #[serde(default)]
    pub max_parallel_installations: Option<u32>,

    #[serde(default)]
    pub logging: Option<LogConfig>,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LogConfig {
    #[serde(default)]
    pub enabled: bool,

    pub directory: PathBuf,

    #[serde(default = "default_max_files")]
    pub max_files: usize,

    #[serde(default = "default_max_size")]
    pub max_size: usize,
}

const fn default_max_files() -> usize {
    LOG_MAX_FILES_DEFAULT
}

const fn default_max_size() -> usize {
    LOG_MAX_SIZE_DEFAULT
}

/// Comprehensive application configuration that combines file config and CLI args
#[derive(Debug, Clone)]
pub struct AppConfig {
    // Core settings
    environment: String,
    package_directory: PathBuf,

    // UI settings
    verbose: bool,
    use_colors: bool,
    use_unicode: bool,

    // Execution settings
    command_timeout: Duration,
    max_parallel: usize,
    stop_on_error: bool,

    // Logging settings
    logging_enabled: bool,
    log_directory: Option<PathBuf>,
    log_max_files: usize,
    log_max_size: usize,
}

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

// Renamed from Config to FileConfig for clarity
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
        if self.environment.is_empty() {
            return Err(ConfigValidationError::EmptyField("environment".to_string()));
        }

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

        // Validate optional parameters
        if let Some(timeout) = self.command_timeout {
            if timeout == 0 {
                return Err(ConfigValidationError::InvalidCommandTimeout(
                    "Command timeout must be greater than 0".to_string(),
                ));
            }
        }

        if let Some(max_parallel) = self.max_parallel_installations {
            if max_parallel == 0 {
                return Err(ConfigValidationError::InvalidMaxParallel(
                    "Max parallel installations must be greater than 0".to_string(),
                ));
            }
        }

        // Validate logging config if present
        if let Some(log_config) = &self.logging {
            if log_config.enabled {
                if log_config.directory.as_os_str().is_empty() {
                    return Err(ConfigValidationError::InvalidLogConfig(
                        "Log directory must be specified when logging is enabled".to_string(),
                    ));
                }

                if log_config.max_files == 0 {
                    return Err(ConfigValidationError::InvalidLogConfig(
                        "Max log files must be greater than 0".to_string(),
                    ));
                }

                if log_config.max_size == 0 {
                    return Err(ConfigValidationError::InvalidLogConfig(
                        "Max log size must be greater than 0".to_string(),
                    ));
                }
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

impl AppConfig {
    pub fn environment(&self) -> &str {
        &self.environment
    }

    pub fn package_directory(&self) -> &PathBuf {
        &self.package_directory
    }

    pub fn verbose(&self) -> bool {
        self.verbose
    }

    pub fn use_colors(&self) -> bool {
        self.use_colors
    }

    pub fn use_unicode(&self) -> bool {
        self.use_unicode
    }

    pub fn command_timeout(&self) -> Duration {
        self.command_timeout
    }

    pub fn max_parallel(&self) -> usize {
        self.max_parallel
    }

    pub fn stop_on_error(&self) -> bool {
        self.stop_on_error
    }

    pub fn logging_enabled(&self) -> bool {
        self.logging_enabled
    }

    pub fn log_directory(&self) -> Option<&PathBuf> {
        self.log_directory.as_ref()
    }

    pub fn log_max_files(&self) -> usize {
        self.log_max_files
    }

    pub fn log_max_size(&self) -> usize {
        self.log_max_size
    }

    /// Create a new AppConfig with default values
    pub fn new(environment: String, package_directory: PathBuf) -> Self {
        Self {
            environment,
            package_directory,
            verbose: VERBOSE_DEFAULT,
            use_colors: USE_COLORS_DEFAULT,
            use_unicode: USE_UNICODE_DEFAULT,
            command_timeout: Duration::from_secs(COMMAND_TIMEOUT_DEFAULT),
            max_parallel: num_cpus::get(),
            stop_on_error: STOP_ON_ERROR_DEFAULT,
            logging_enabled: LOGGING_ENABLED_DEFAULT,
            log_directory: None,
            log_max_files: LOG_MAX_FILES_DEFAULT,
            log_max_size: LOG_MAX_SIZE_DEFAULT,
        }
    }

    /// Create an AppConfig from a FileConfig
    pub fn from_file_config(config: FileConfig) -> Self {
        Self {
            environment: config.environment,
            package_directory: config.package_directory,
            verbose: VERBOSE_DEFAULT,
            use_colors: USE_COLORS_DEFAULT,
            use_unicode: USE_UNICODE_DEFAULT,
            command_timeout: Duration::from_secs(
                config.command_timeout.unwrap_or(COMMAND_TIMEOUT_DEFAULT),
            ),
            max_parallel: config
                .max_parallel_installations
                .map(|v| v as usize)
                .unwrap_or(num_cpus::get()),
            stop_on_error: config.stop_on_error.unwrap_or(STOP_ON_ERROR_DEFAULT),
            logging_enabled: config.logging.as_ref().is_some_and(|l| l.enabled),
            log_directory: config.logging.as_ref().map(|l| l.directory.clone()),
            log_max_files: config.logging.as_ref().map_or(10, |l| l.max_files),
            log_max_size: config.logging.as_ref().map_or(10, |l| l.max_size),
        }
    }

    /// Apply CLI arguments to override configuration
    pub fn apply_cli_args(
        mut self,
        environment: Option<String>,
        package_directory: Option<PathBuf>,
        verbose: bool,
        no_color: bool,
    ) -> Self {
        // Override environment if specified
        if let Some(env) = environment {
            self.environment = env;
        }

        // Override package directory if specified
        if let Some(dir) = package_directory {
            self.package_directory = dir;
        }

        // Apply UI settings
        self.verbose = verbose;
        self.use_colors = !no_color;

        self
    }

    /// Full validation for the AppConfig
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        // Validate core settings
        if self.environment.is_empty() {
            return Err(ConfigValidationError::EmptyField("environment".to_string()));
        }

        if self.package_directory.as_os_str().is_empty() {
            return Err(ConfigValidationError::EmptyField(
                "package_directory".to_string(),
            ));
        }

        // Validate the package directory path
        let package_dir = self.package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        let expanded_path = Path::new(expanded_path.as_ref());

        if !expanded_path.is_absolute() {
            return Err(ConfigValidationError::InvalidPackageDirectory(
                "Package directory must be an absolute path".to_string(),
            ));
        }

        // Validate execution settings
        if self.command_timeout.as_secs() == 0 {
            return Err(ConfigValidationError::InvalidCommandTimeout(
                "Command timeout must be greater than 0".to_string(),
            ));
        }

        if self.max_parallel == 0 {
            return Err(ConfigValidationError::InvalidMaxParallel(
                "Max parallel installations must be greater than 0".to_string(),
            ));
        }

        // Validate logging settings if enabled
        if self.logging_enabled {
            if self.log_directory.is_none()
                || self.log_directory.as_ref().unwrap().as_os_str().is_empty()
            {
                return Err(ConfigValidationError::InvalidLogConfig(
                    "Log directory must be specified when logging is enabled".to_string(),
                ));
            }

            if self.log_max_files == 0 {
                return Err(ConfigValidationError::InvalidLogConfig(
                    "Max log files must be greater than 0".to_string(),
                ));
            }

            if self.log_max_size == 0 {
                return Err(ConfigValidationError::InvalidLogConfig(
                    "Max log size must be greater than 0".to_string(),
                ));
            }
        }

        Ok(())
    }

    /// Validate minimal requirements for commands that don't need a full config
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

    /// Get the expanded package directory path
    pub fn expanded_package_directory(&self) -> PathBuf {
        let package_dir = self.package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        PathBuf::from(expanded_path.as_ref())
    }

    /// Resolve environment configuration for a package
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
pub struct ConfigBuilder {
    environment: String,
    package_directory: PathBuf,
    command_timeout: Option<u64>,
    stop_on_error: Option<bool>,
    max_parallel_installations: Option<u32>,
    logging: Option<LogConfig>,
}

impl ConfigBuilder {
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

    pub fn max_parallel(mut self, max: u32) -> Self {
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

/// Builder pattern for AppConfig testing
pub struct AppConfigBuilder {
    environment: String,
    package_directory: PathBuf,
    verbose: bool,
    use_colors: bool,
    use_unicode: bool,
    command_timeout: u64,
    max_parallel: usize,
    stop_on_error: bool,
    logging_enabled: bool,
    log_directory: Option<PathBuf>,
    log_max_files: usize,
    log_max_size: usize,
}

impl AppConfigBuilder {
    pub fn environment(mut self, environment: &str) -> Self {
        self.environment = environment.to_string();
        self
    }

    pub fn package_directory<D>(mut self, package_directory: D) -> Self
    where
        D: AsRef<std::ffi::OsStr>,
    {
        self.package_directory = PathBuf::from(package_directory.as_ref());
        self
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn use_colors(mut self, use_colors: bool) -> Self {
        self.use_colors = use_colors;
        self
    }

    pub fn use_unicode(mut self, use_unicode: bool) -> Self {
        self.use_unicode = use_unicode;
        self
    }

    pub fn command_timeout(mut self, timeout: u64) -> Self {
        self.command_timeout = timeout;
        self
    }

    pub fn max_parallel(mut self, max: usize) -> Self {
        self.max_parallel = max;
        self
    }

    pub fn stop_on_error(mut self, stop: bool) -> Self {
        self.stop_on_error = stop;
        self
    }

    pub fn logging_enabled(mut self, enabled: bool) -> Self {
        self.logging_enabled = enabled;
        self
    }

    pub fn log_directory<D>(mut self, directory: D) -> Self
    where
        D: AsRef<std::ffi::OsStr>,
    {
        self.log_directory = Some(PathBuf::from(directory.as_ref()));
        self
    }

    pub fn log_max_files(mut self, max: usize) -> Self {
        self.log_max_files = max;
        self
    }

    pub fn log_max_size(mut self, max: usize) -> Self {
        self.log_max_size = max;
        self
    }

    pub fn build(self) -> AppConfig {
        AppConfig {
            environment: self.environment,
            package_directory: self.package_directory,
            verbose: self.verbose,
            use_colors: self.use_colors,
            use_unicode: self.use_unicode,
            command_timeout: Duration::from_secs(self.command_timeout),
            max_parallel: self.max_parallel,
            stop_on_error: self.stop_on_error,
            logging_enabled: self.logging_enabled,
            log_directory: self.log_directory,
            log_max_files: self.log_max_files,
            log_max_size: self.log_max_size,
        }
    }
}

impl Default for AppConfigBuilder {
    fn default() -> Self {
        Self {
            environment: String::default(),
            package_directory: PathBuf::new(),
            verbose: VERBOSE_DEFAULT,
            use_colors: USE_COLORS_DEFAULT,
            use_unicode: USE_UNICODE_DEFAULT,
            command_timeout: COMMAND_TIMEOUT_DEFAULT,
            max_parallel: num_cpus::get(),
            stop_on_error: STOP_ON_ERROR_DEFAULT,
            logging_enabled: LOGGING_ENABLED_DEFAULT,
            log_directory: None,
            log_max_files: LOG_MAX_FILES_DEFAULT,
            log_max_size: LOG_MAX_SIZE_DEFAULT,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_config_builder() {
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .verbose(true)
            .use_colors(false)
            .command_timeout(120)
            .max_parallel(8)
            .build();

        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, PathBuf::from("/test/path"));
        assert!(config.verbose);
        assert!(!config.use_colors);
        assert_eq!(config.command_timeout, Duration::from_secs(120));
        assert_eq!(config.max_parallel, 8);
    }

    #[test]
    fn test_app_config_from_file_config() {
        let file_config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .command_timeout(120)
            .max_parallel(8)
            .stop_on_error(false)
            .build();

        let app_config = AppConfig::from_file_config(file_config);

        assert_eq!(app_config.environment, "test-env");
        assert_eq!(app_config.package_directory, PathBuf::from("/test/path"));
        assert!(!app_config.verbose); // Default is false
        assert!(app_config.use_colors); // Default is true
        assert_eq!(app_config.command_timeout, Duration::from_secs(120));
        assert_eq!(app_config.max_parallel, 8);
        assert!(!app_config.stop_on_error);
    }

    #[test]
    fn test_app_config_apply_cli_args() {
        let config = AppConfigBuilder::default()
            .environment("file-env")
            .package_directory("/file/path")
            .build();

        let updated = config.apply_cli_args(
            Some("cli-env".to_string()),
            Some(PathBuf::from("/cli/path")),
            true,
            true,
        );

        assert_eq!(updated.environment, "cli-env");
        assert_eq!(updated.package_directory, PathBuf::from("/cli/path"));
        assert!(updated.verbose);
        assert!(!updated.use_colors);
    }

    #[test]
    fn test_app_config_validation() {
        // Valid config
        let valid_config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .build();

        assert!(valid_config.validate().is_ok());

        // Empty environment
        let invalid_config = AppConfigBuilder::default()
            .environment("")
            .package_directory("/test/path")
            .build();

        assert!(matches!(
            invalid_config.validate(),
            Err(ConfigValidationError::EmptyField(field)) if field == "environment"
        ));

        // Zero command timeout
        let invalid_config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .command_timeout(0)
            .build();

        assert!(matches!(
            invalid_config.validate(),
            Err(ConfigValidationError::InvalidCommandTimeout(_))
        ));

        // Zero max parallel
        let invalid_config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .max_parallel(0)
            .build();

        assert!(matches!(
            invalid_config.validate(),
            Err(ConfigValidationError::InvalidMaxParallel(_))
        ));

        // Invalid logging config
        let invalid_config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .logging_enabled(true)
            .log_max_files(0)
            .build();

        assert!(matches!(
            invalid_config.validate(),
            Err(ConfigValidationError::InvalidLogConfig(_))
        ));
    }
}
