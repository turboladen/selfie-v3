use std::{ffi::OsStr, path::Path};

use super::{AppConfig, ConfigValidationError, FileConfig};

pub(super) trait ValidateConfig {
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

    fn validate_command_timeout(command_timeout_secs: u64) -> Result<(), ConfigValidationError> {
        if command_timeout_secs == 0 {
            Err(ConfigValidationError::InvalidCommandTimeout(
                "Command timeout must be greater than 0".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn validate_max_parallel_installations(
        max_parallel: usize,
    ) -> Result<(), ConfigValidationError> {
        if max_parallel == 0 {
            Err(ConfigValidationError::InvalidMaxParallel(
                "Max parallel installations must be greater than 0".to_string(),
            ))
        } else {
            Ok(())
        }
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

    fn validate_log_max_files(max_files: usize) -> Result<(), ConfigValidationError> {
        if max_files == 0 {
            Err(ConfigValidationError::InvalidLogConfig(
                "Max log files must be greater than 0".to_string(),
            ))
        } else {
            Ok(())
        }
    }

    fn validate_log_max_size(max_size: usize) -> Result<(), ConfigValidationError> {
        if max_size == 0 {
            Err(ConfigValidationError::InvalidLogConfig(
                "Max log size must be greater than 0".to_string(),
            ))
        } else {
            Ok(())
        }
    }
}

impl ValidateConfig for FileConfig {}
impl ValidateConfig for AppConfig {}
