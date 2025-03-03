// src/domain/package.rs
// Core package entity and related types

use std::{collections::HashMap, path::PathBuf};

use serde::{Deserialize, Serialize};
use thiserror::Error;

/// Core package entity representing a package definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Package {
    /// Package name
    pub name: String,

    /// Package version (for the selfie package file, not the underlying package)
    pub version: String,

    /// Optional homepage URL
    #[serde(default)]
    pub homepage: Option<String>,

    /// Optional package description
    #[serde(default)]
    pub description: Option<String>,

    /// Map of environment configurations
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,

    /// Path to the package file (not serialized/deserialized)
    #[serde(skip, default)]
    pub path: Option<PathBuf>,
}

/// Configuration for a specific environment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    /// Command to install the package
    pub install: String,

    /// Optional command to check if the package is already installed
    #[serde(default)]
    pub check: Option<String>,

    /// Dependencies that must be installed before this package
    #[serde(default)]
    pub dependencies: Vec<String>,
}

/// Errors related to package validation
#[derive(Error, Debug, PartialEq)]
pub enum PackageValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Empty field: {0}")]
    EmptyField(String),

    #[error("Environment '{0}' not supported by package")]
    EnvironmentNotSupported(String),

    #[error("YAML parsing error: {0}")]
    YamlParseError(String),

    #[error("File system error: {0}")]
    FileSystemError(String),
}

/// Errors related to package parsing
#[derive(Error, Debug)]
pub enum PackageParseError {
    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("File system error: {0}")]
    FileSystemError(String),
}

impl Package {
    /// Create a new package with the specified attributes
    pub fn new(
        name: String,
        version: String,
        homepage: Option<String>,
        description: Option<String>,
        environments: HashMap<String, EnvironmentConfig>,
    ) -> Self {
        Self {
            name,
            version,
            homepage,
            description,
            environments,
            path: None,
        }
    }

    /// Associate the package with a file path
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    /// Validate that the package metadata is complete and valid
    pub fn validate(&self) -> Result<(), PackageValidationError> {
        if self.name.is_empty() {
            return Err(PackageValidationError::EmptyField("name".to_string()));
        }

        if self.version.is_empty() {
            return Err(PackageValidationError::EmptyField("version".to_string()));
        }

        if self.environments.is_empty() {
            return Err(PackageValidationError::MissingField(
                "environments".to_string(),
            ));
        }

        for (env_name, env_config) in &self.environments {
            if env_name.is_empty() {
                return Err(PackageValidationError::EmptyField(
                    "environment name".to_string(),
                ));
            }

            if env_config.install.is_empty() {
                return Err(PackageValidationError::EmptyField(format!(
                    "install command for environment '{}'",
                    env_name
                )));
            }
        }
        Ok(())
    }

    /// Resolve an environment configuration by name
    pub fn resolve_environment(
        &self,
        environment_name: &str,
    ) -> Result<&EnvironmentConfig, PackageValidationError> {
        self.environments.get(environment_name).ok_or_else(|| {
            if self.environments.is_empty() {
                PackageValidationError::MissingField("environments".to_string())
            } else {
                PackageValidationError::EnvironmentNotSupported(environment_name.to_string())
            }
        })
    }
}
// Builder pattern for testing
#[cfg(test)]
#[derive(Default)]
pub struct PackageBuilder {
    name: String,
    version: String,
    homepage: Option<String>,
    description: Option<String>,
    environments: HashMap<String, EnvironmentConfig>,
}

#[cfg(test)]
impl PackageBuilder {
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub fn homepage(mut self, homepage: &str) -> Self {
        self.homepage = Some(homepage.to_string());
        self
    }

    pub fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub fn environment<T>(mut self, name: T, install_command: &str) -> Self
    where
        T: ToString,
    {
        self.environments.insert(
            name.to_string(),
            EnvironmentConfig {
                install: install_command.to_string(),
                check: None,
                dependencies: Vec::new(),
            },
        );
        self
    }

    pub fn environment_with_check<T>(
        mut self,
        name: T,
        install_command: &str,
        check_command: &str,
    ) -> Self
    where
        T: ToString,
    {
        self.environments.insert(
            name.to_string(),
            EnvironmentConfig {
                install: install_command.to_string(),
                check: Some(check_command.to_string()),
                dependencies: Vec::new(),
            },
        );
        self
    }

    pub fn environment_with_dependencies<T>(
        mut self,
        name: T,
        install_command: &str,
        dependencies: Vec<&str>,
    ) -> Self
    where
        T: ToString,
    {
        self.environments.insert(
            name.to_string(),
            EnvironmentConfig {
                install: install_command.to_string(),
                check: None,
                dependencies: dependencies.iter().map(|&s| s.to_string()).collect(),
            },
        );
        self
    }

    pub fn build(self) -> Package {
        Package::new(
            self.name,
            self.version,
            self.homepage,
            self.description,
            self.environments,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_package_node() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        assert_eq!(package.name, "test-package");
        assert_eq!(package.version, "1.0.0");
        assert_eq!(package.environments.len(), 1);
        assert_eq!(
            package.environments.get("test-env").unwrap().install,
            "test install"
        );
    }

    #[test]
    fn test_create_package_with_metadata() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .homepage("https://example.com")
            .description("Test package description")
            .environment("test-env", "test install")
            .build();

        assert_eq!(package.name, "test-package");
        assert_eq!(package.version, "1.0.0");
        assert_eq!(package.homepage, Some("https://example.com".to_string()));
        assert_eq!(
            package.description,
            Some("Test package description".to_string())
        );
        assert_eq!(package.environments.len(), 1);
    }

    #[test]
    fn test_validate_valid_package() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        assert!(package.validate().is_ok());
    }

    #[test]
    fn test_validate_missing_fields() {
        let package = PackageBuilder::default().build();

        assert_eq!(
            package.validate(),
            Err(PackageValidationError::EmptyField("name".to_string()))
        );
    }

    #[test]
    fn test_validate_empty_fields() {
        let package = PackageBuilder::default()
            .name("")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        assert_eq!(
            package.validate(),
            Err(PackageValidationError::EmptyField("name".to_string()))
        );
    }

    #[test]
    fn test_validate_empty_environment() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "")
            .build();

        assert_eq!(
            package.validate(),
            Err(PackageValidationError::EmptyField(
                "install command for environment 'test-env'".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_environment_valid() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .environment("prod-env", "prod install")
            .build();

        let result = package.resolve_environment("test-env");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().install, "test install");
    }

    #[test]
    fn test_resolve_environment_case_insensitive() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("Test-Env", "test install")
            .build();

        let result = package.resolve_environment("test-env");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_environment_not_found() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        let result = package.resolve_environment("prod-env");
        assert_eq!(
            result,
            Err(PackageValidationError::EnvironmentNotSupported(
                "prod-env".to_string()
            ))
        );
    }

    #[test]
    fn test_resolve_environment_empty() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .build();

        let result = package.resolve_environment("test-env");
        assert_eq!(
            result,
            Err(PackageValidationError::MissingField(
                "environments".to_string()
            ))
        );
    }
}
