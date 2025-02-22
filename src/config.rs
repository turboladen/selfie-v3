// src/config.rs

use std::path::{Path, PathBuf};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct Config {
    pub environment: String,
    pub package_directory: PathBuf,
}

#[derive(Error, Debug, PartialEq)]
pub enum ConfigValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Empty field: {0}")]
    EmptyField(String),

    #[error("Invalid package directory: {0}")]
    InvalidPackageDirectory(String),
}

impl Config {
    pub fn new(environment: String, package_directory: PathBuf) -> Self {
        Self {
            environment,
            package_directory,
        }
    }

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

        Ok(())
    }

    pub fn expanded_package_directory(&self) -> PathBuf {
        let package_dir = self.package_directory.to_string_lossy();
        let expanded_path = shellexpand::tilde(&package_dir);
        PathBuf::from(expanded_path.as_ref())
    }
}

// Builder pattern for testing
#[cfg(test)]
#[derive(Default)]
pub struct ConfigBuilder {
    environment: String,
    package_directory: PathBuf,
}

#[cfg(test)]
impl ConfigBuilder {
    pub fn environment(mut self, environment: &str) -> Self {
        self.environment = environment.to_string();
        self
    }

    pub fn package_directory(mut self, package_directory: &str) -> Self {
        self.package_directory = PathBuf::from(package_directory);
        self
    }

    pub fn build(self) -> Config {
        Config::new(self.environment, self.package_directory)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_config() {
        let config = Config::new("test-env".to_string(), PathBuf::from("/test/path"));
        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, PathBuf::from("/test/path"));
    }

    #[test]
    fn test_validate_valid_config() {
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .build();

        assert!(config.validate().is_ok());
    }

    #[test]
    fn test_validate_empty_environment() {
        let config = ConfigBuilder::default()
            .environment("")
            .package_directory("/test/path")
            .build();

        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::EmptyField("environment".to_string()))
        );
    }

    #[test]
    fn test_validate_empty_package_directory() {
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("")
            .build();

        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::EmptyField(
                "package_directory".to_string()
            ))
        );
    }

    #[test]
    fn test_validate_relative_package_directory() {
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("relative/path")
            .build();

        assert_eq!(
            config.validate(),
            Err(ConfigValidationError::InvalidPackageDirectory(
                "Package directory must be an absolute path".to_string()
            ))
        );
    }

    #[test]
    fn test_expanded_package_directory() {
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("~/test/path")
            .build();

        let home_dir = dirs::home_dir().unwrap();
        let expected_path = home_dir.join("test/path");
        assert_eq!(config.expanded_package_directory(), expected_path);
    }
}
