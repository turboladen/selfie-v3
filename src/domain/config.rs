// src/domain/config.rs

use std::{
    ffi::OsStr,
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
    time::Duration,
};

use serde::Deserialize;
use thiserror::Error;

use crate::{
    domain::package::{EnvironmentConfig, Package, PackageValidationError},
    ports::application::ApplicationArguments,
};

const VERBOSE_DEFAULT: bool = false;
const USE_COLORS_DEFAULT: bool = true;
const USE_UNICODE_DEFAULT: bool = true;
const STOP_ON_ERROR_DEFAULT: bool = true;

/// Comprehensive application configuration that combines file config and CLI args
#[derive(Debug, Clone, Deserialize)]
pub struct AppConfig {
    // Core settings
    pub(crate) environment: String,
    pub(crate) package_directory: PathBuf,

    // UI settings
    #[serde(default)]
    pub(crate) verbose: bool,

    #[serde(default = "default_use_colors")]
    pub(crate) use_colors: bool,

    #[serde(default = "default_use_unicode")]
    pub(crate) use_unicode: bool,

    // Execution settings
    // command_timeout: Duration,
    #[serde(default = "default_command_timeout")]
    pub(crate) command_timeout: NonZeroU64,

    #[serde(default = "default_stop_on_error")]
    pub(crate) stop_on_error: bool,

    #[serde(default = "default_max_parallel")]
    pub(crate) max_parallel_installations: NonZeroUsize,

    // Logging settings
    #[serde(default)]
    pub(crate) logging: LoggingConfig,
}

#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct LoggingConfig {
    #[serde(default)]
    pub enabled: bool,

    #[serde(default)]
    pub directory: Option<PathBuf>,

    #[serde(default = "default_log_max_files")]
    pub max_files: NonZeroUsize,

    #[serde(default = "default_log_max_size")]
    pub max_size: NonZeroUsize,
}

impl Default for LoggingConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            directory: None,
            max_files: default_log_max_files(),
            max_size: default_log_max_size(),
        }
    }
}

const fn default_log_max_files() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(10) }
}

const fn default_log_max_size() -> NonZeroUsize {
    unsafe { NonZeroUsize::new_unchecked(10) }
}

fn default_command_timeout() -> NonZeroU64 {
    unsafe { NonZeroU64::new_unchecked(60) }
}
fn default_stop_on_error() -> bool {
    true
}
fn default_max_parallel() -> NonZeroUsize {
    NonZeroUsize::new(num_cpus::get()).unwrap_or_else(|| unsafe { NonZeroUsize::new_unchecked(4) })
}
fn default_use_colors() -> bool {
    true
}
fn default_use_unicode() -> bool {
    true
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
        Duration::from_secs(self.command_timeout.into())
    }

    pub fn max_parallel(&self) -> NonZeroUsize {
        self.max_parallel_installations
    }

    pub fn stop_on_error(&self) -> bool {
        self.stop_on_error
    }

    pub fn logging_enabled(&self) -> bool {
        self.logging.enabled
    }

    pub fn log_directory(&self) -> Option<&PathBuf> {
        self.logging.directory.as_ref()
    }

    pub fn log_max_files(&self) -> NonZeroUsize {
        self.logging.max_files
    }

    pub fn log_max_size(&self) -> NonZeroUsize {
        self.logging.max_size
    }

    /// Create a new AppConfig with default values
    pub fn new(environment: String, package_directory: PathBuf) -> Self {
        Self {
            environment,
            package_directory,
            verbose: VERBOSE_DEFAULT,
            use_colors: USE_COLORS_DEFAULT,
            use_unicode: USE_UNICODE_DEFAULT,
            command_timeout: default_command_timeout(),
            max_parallel_installations: default_max_parallel(),
            stop_on_error: STOP_ON_ERROR_DEFAULT,
            logging: LoggingConfig::default(),
        }
    }

    /// Apply CLI arguments to override configuration
    pub fn apply_cli_args(mut self, args: &ApplicationArguments) -> Self {
        // Override environment if specified
        if let Some(env) = args.environment.as_ref() {
            self.environment = env.clone();
        }

        // Override package directory if specified
        if let Some(dir) = args.package_directory.as_ref() {
            self.package_directory = dir.clone();
        }

        // Apply UI settings
        self.verbose = args.verbose;
        self.use_colors = !args.no_color;

        self
    }

    /// Full validation for the AppConfig
    pub fn validate(&self) -> Result<(), ConfigValidationError> {
        Self::validate_environment(&self.environment)?;
        Self::validate_package_directory(&self.package_directory)?;

        // Validate logging settings if enabled
        if self.logging.enabled {
            Self::validate_log_directory(
                self.logging
                    .directory
                    .as_ref()
                    .map(|ld| ld.as_os_str())
                    .unwrap_or_default(),
            )?;
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

trait ValidateConfig {
    fn validate_environment(environment: &str) -> Result<(), ConfigValidationError> {
        if environment.is_empty() {
            Err(ConfigValidationError::EmptyField("environment".to_string()))
        } else {
            Ok(())
        }
    }

    fn validate_package_directory(package_directory: &Path) -> Result<(), ConfigValidationError> {
        if package_directory.as_os_str().is_empty() {
            return Err(ConfigValidationError::EmptyField(
                "package_directory".to_string(),
            ));
        }

        // Validate the package directory path
        let package_dir = package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        let expanded_path = Path::new(expanded_path.as_ref());

        if !expanded_path.is_absolute() {
            return Err(ConfigValidationError::InvalidPackageDirectory(
                "Package directory must be an absolute path".to_string(),
            ));
        }

        Ok(())
    }

    fn validate_log_directory(log_directory: &OsStr) -> Result<(), ConfigValidationError> {
        if log_directory.is_empty() {
            Err(ConfigValidationError::InvalidLogConfig(
                "Log directory must be specified when logging is enabled".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

impl ValidateConfig for AppConfig {}

/// Builder pattern for AppConfig testing
pub struct AppConfigBuilder {
    environment: String,
    package_directory: PathBuf,
    verbose: bool,
    use_colors: bool,
    use_unicode: bool,
    command_timeout: NonZeroU64,
    max_parallel: NonZeroUsize,
    stop_on_error: bool,
    logging: LoggingConfig,
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

    pub fn command_timeout(mut self, timeout: NonZeroU64) -> Self {
        self.command_timeout = timeout;
        self
    }

    pub fn command_timeout_unchecked(mut self, timeout: u64) -> Self {
        self.command_timeout = NonZeroU64::new(timeout).unwrap();
        self
    }

    pub fn max_parallel(mut self, max: NonZeroUsize) -> Self {
        self.max_parallel = max;
        self
    }

    pub fn max_parallel_unchecked(mut self, max: usize) -> Self {
        self.max_parallel = NonZeroUsize::new(max).unwrap();
        self
    }

    pub fn stop_on_error(mut self, stop: bool) -> Self {
        self.stop_on_error = stop;
        self
    }

    pub fn logging_enabled(mut self, enabled: bool) -> Self {
        self.logging.enabled = enabled;
        self
    }

    pub fn log_directory<D>(mut self, directory: D) -> Self
    where
        D: AsRef<std::ffi::OsStr>,
    {
        self.logging.directory = Some(PathBuf::from(directory.as_ref()));
        self
    }

    pub fn log_max_files(mut self, max: NonZeroUsize) -> Self {
        self.logging.max_files = max;
        self
    }

    pub fn log_max_files_unchecked(mut self, max: usize) -> Self {
        self.logging.max_files = NonZeroUsize::new(max).unwrap();
        self
    }

    pub fn log_max_size(mut self, max: NonZeroUsize) -> Self {
        self.logging.max_size = max;
        self
    }

    pub fn log_max_size_unchecked(mut self, max: usize) -> Self {
        self.logging.max_size = NonZeroUsize::new(max).unwrap();
        self
    }

    pub fn build(self) -> AppConfig {
        AppConfig {
            environment: self.environment,
            package_directory: self.package_directory,
            verbose: self.verbose,
            use_colors: self.use_colors,
            use_unicode: self.use_unicode,
            command_timeout: self.command_timeout,
            max_parallel_installations: self.max_parallel,
            stop_on_error: self.stop_on_error,
            logging: LoggingConfig {
                enabled: self.logging.enabled,
                directory: self.logging.directory,
                max_files: self.logging.max_files,
                max_size: self.logging.max_size,
            },
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
            command_timeout: default_command_timeout(),
            max_parallel: default_max_parallel(),
            stop_on_error: STOP_ON_ERROR_DEFAULT,
            logging: LoggingConfig::default(),
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::domain::application::commands::{ApplicationCommand, PackageCommand};

    use super::*;

    #[test]
    fn test_app_config_builder() {
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .verbose(true)
            .use_colors(false)
            .command_timeout_unchecked(120)
            .max_parallel_unchecked(8)
            .build();

        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, PathBuf::from("/test/path"));
        assert!(config.verbose);
        assert!(!config.use_colors);
        assert_eq!(config.command_timeout(), Duration::from_secs(120));
        assert_eq!(
            config.max_parallel_installations,
            NonZeroUsize::new(8).unwrap()
        );
    }

    #[test]
    fn test_app_config_apply_cli_args() {
        let config = AppConfigBuilder::default()
            .environment("file-env")
            .package_directory("/file/path")
            .build();

        let args = ApplicationArguments {
            environment: Some("cli-env".to_string()),
            package_directory: Some(PathBuf::from("/cli/path")),
            verbose: true,
            no_color: true,
            command: ApplicationCommand::Package(PackageCommand::List),
        };
        let updated = config.apply_cli_args(&args);

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
    }
}
