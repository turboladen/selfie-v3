use std::{
    num::{NonZeroU64, NonZeroUsize},
    path::{Path, PathBuf},
    time::Duration,
};

use validator::Validate;

use crate::domain::package::{EnvironmentConfig, Package, PackageValidationError};

use super::{
    num_cpus_default, ConfigValidationError, ConfigValidationErrors, FileConfig,
    COMMAND_TIMEOUT_DEFAULT, LOGGING_ENABLED_DEFAULT, LOG_MAX_FILES_DEFAULT, LOG_MAX_SIZE_DEFAULT,
    STOP_ON_ERROR_DEFAULT, USE_COLORS_DEFAULT, USE_UNICODE_DEFAULT, VERBOSE_DEFAULT,
};

pub struct LogConfigRef<'a> {
    enabled: bool,
    directory: Option<&'a Path>,
    max_files: NonZeroUsize,
    max_size: NonZeroUsize,
}

impl<'a> From<&'a AppConfig> for LogConfigRef<'a> {
    fn from(value: &'a AppConfig) -> Self {
        Self {
            enabled: value.logging_enabled,
            directory: value.log_directory.as_deref(),
            max_files: value.log_max_files,
            max_size: value.log_max_size,
        }
    }
}

/// Comprehensive application configuration that combines file config and CLI args
#[derive(Debug, Clone, Validate)]
#[validate(context = "LogConfigRef<'v_a>")]
pub struct AppConfig {
    #[validate(length(min = 1, message = "Environment name cannot be empty"))]
    environment: String,

    #[validate(custom(function = "super::validate_path", code = "InvalidPackageDirectory"))]
    package_directory: PathBuf,

    // UI settings
    verbose: bool,
    use_colors: bool,
    use_unicode: bool,

    // Execution settings
    #[validate(custom(function = "validate_duration"))]
    command_timeout: Duration,

    max_parallel: NonZeroUsize,
    stop_on_error: bool,

    // Logging settings
    #[validate(custom(function = "validate_logging_enabled", use_context))]
    logging_enabled: bool,

    #[validate(custom(function = "validate_log_directory", use_context))]
    log_directory: Option<PathBuf>,

    log_max_files: NonZeroUsize,

    log_max_size: NonZeroUsize,
}

fn validate_duration(duration: &Duration) -> Result<(), validator::ValidationError> {
    if duration.as_secs() == 0 {
        Err(validator::ValidationError::new(
            "Command timeout must be greater than 0",
        ))
    } else {
        Ok(())
    }
}

fn validate_logging_enabled(
    logging_enabled: &bool,
    log_config: &LogConfigRef<'_>,
) -> Result<(), validator::ValidationError> {
    if *logging_enabled && log_config.directory.is_none() {
        return Err(validator::ValidationError::new(
            "Log directory must be specified when logging is enabled",
        ));
    }

    Ok(())
}

/// Custom validator for log directory when logging is enabled
fn validate_log_directory(
    log_dir: &Path,
    log_config: &LogConfigRef<'_>,
) -> Result<(), validator::ValidationError> {
    if log_config.enabled {
        super::validate_path(log_dir)
    } else {
        Ok(())
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

    pub fn max_parallel(&self) -> NonZeroUsize {
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

    pub fn log_max_files(&self) -> NonZeroUsize {
        self.log_max_files
    }

    pub fn log_max_size(&self) -> NonZeroUsize {
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
            command_timeout: Duration::from_secs(COMMAND_TIMEOUT_DEFAULT.into()),
            max_parallel: num_cpus_default(),
            stop_on_error: STOP_ON_ERROR_DEFAULT,
            logging_enabled: LOGGING_ENABLED_DEFAULT,
            log_directory: None,
            log_max_files: LOG_MAX_FILES_DEFAULT,
            log_max_size: LOG_MAX_SIZE_DEFAULT,
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
    pub fn validate_full(&self) -> Result<(), ConfigValidationErrors> {
        Ok(<Self as validator::ValidateArgs>::validate_with_args(
            self,
            &LogConfigRef::from(self),
        )?)
    }

    /// Validate minimal requirements for commands that don't need a full config
    pub fn validate_minimal(&self) -> Result<(), ConfigValidationError> {
        if self.package_directory.as_os_str().is_empty() {
            return Err(ConfigValidationError::EmptyField {
                field: "package_directory".to_string(),
            });
        }

        // Expand the package directory path
        let package_dir = self.package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        let expanded_path = Path::new(expanded_path.as_ref());

        if !expanded_path.is_absolute() {
            return Err(ConfigValidationError::InvalidPackageDirectory(
                "Expanded package directory must be an absolute path".to_string(),
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

impl From<FileConfig> for AppConfig {
    fn from(config: FileConfig) -> Self {
        Self {
            environment: config.environment,
            package_directory: config.package_directory,
            verbose: VERBOSE_DEFAULT,
            use_colors: USE_COLORS_DEFAULT,
            use_unicode: USE_UNICODE_DEFAULT,
            command_timeout: Duration::from_secs(
                config
                    .command_timeout
                    .unwrap_or(COMMAND_TIMEOUT_DEFAULT)
                    .into(),
            ),
            max_parallel: config
                .max_parallel_installations
                .unwrap_or(num_cpus_default()),
            stop_on_error: config.stop_on_error.unwrap_or(STOP_ON_ERROR_DEFAULT),
            logging_enabled: config.logging.as_ref().is_some_and(|l| l.enabled),
            log_directory: config.logging.as_ref().map(|l| l.directory.clone()),
            log_max_files: config
                .logging
                .as_ref()
                .map_or(LOG_MAX_SIZE_DEFAULT, |l| l.max_files),
            log_max_size: config
                .logging
                .as_ref()
                .map_or(LOG_MAX_SIZE_DEFAULT, |l| l.max_size),
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
    command_timeout: NonZeroU64,
    max_parallel: NonZeroUsize,
    stop_on_error: bool,
    logging_enabled: bool,
    log_directory: Option<PathBuf>,
    log_max_files: NonZeroUsize,
    log_max_size: NonZeroUsize,
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

    pub fn max_parallel(mut self, max: NonZeroUsize) -> Self {
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

    pub fn log_max_files(mut self, max: NonZeroUsize) -> Self {
        self.log_max_files = max;
        self
    }

    pub fn log_max_size(mut self, max: NonZeroUsize) -> Self {
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
            command_timeout: Duration::from_secs(self.command_timeout.into()),
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
            max_parallel: num_cpus_default(),
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
    use crate::domain::config::FileConfigBuilder;

    use super::*;

    #[test]
    fn test_app_config_builder() {
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .verbose(true)
            .use_colors(false)
            .command_timeout(NonZeroU64::new(120).unwrap())
            .max_parallel(NonZeroUsize::new(8).unwrap())
            .build();

        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, PathBuf::from("/test/path"));
        assert!(config.verbose);
        assert!(!config.use_colors);
        assert_eq!(config.command_timeout, Duration::from_secs(120));
        assert_eq!(config.max_parallel, NonZeroUsize::new(8).unwrap());
    }

    #[test]
    fn test_app_config_from_file_config() {
        let file_config = FileConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .command_timeout(NonZeroU64::new(120).unwrap())
            .max_parallel(NonZeroUsize::new(8).unwrap())
            .stop_on_error(false)
            .build();

        let app_config = AppConfig::from(file_config);

        assert_eq!(app_config.environment, "test-env");
        assert_eq!(app_config.package_directory, PathBuf::from("/test/path"));
        assert!(!app_config.verbose); // Default is false
        assert!(app_config.use_colors); // Default is true
        assert_eq!(app_config.command_timeout, Duration::from_secs(120));
        assert_eq!(app_config.max_parallel, NonZeroUsize::new(8).unwrap());
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

        assert!(valid_config.validate_full().is_ok());

        // Empty environment
        let invalid_config = AppConfigBuilder::default()
            .environment("")
            .package_directory("/test/path")
            .build();

        let err = invalid_config.validate_full().unwrap_err();
        let e = err.iter().find(|e| {
            matches!(
                e,
                ConfigValidationError::EmptyField { field } if field == "environment"
            )
        });
        assert!(e.is_some());

        // Invalid logging config
        let invalid_config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .logging_enabled(true)
            .log_directory("!@#!")
            .build();

        let err = invalid_config.validate_full().unwrap_err();
        let e = err.iter().find(|e| {
            matches!(e, ConfigValidationError::InvalidPath { field, .. } if field == "log_directory")
        });
        assert!(e.is_some());
    }
}
