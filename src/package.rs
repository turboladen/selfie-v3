// src/package.rs

use std::collections::HashMap;

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct PackageNode {
    pub name: String,
    pub version: String,
    pub environments: HashMap<String, EnvironmentConfig>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct EnvironmentConfig {
    pub install: String,
}

#[derive(Error, Debug, PartialEq)]
pub enum PackageValidationError {
    #[error("Missing required field: {0}")]
    MissingField(String),

    #[error("Empty field: {0}")]
    EmptyField(String),
}

impl PackageNode {
    pub fn new(
        name: String,
        version: String,
        environments: HashMap<String, EnvironmentConfig>,
    ) -> Self {
        Self {
            name,
            version,
            environments,
        }
    }

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
}

// Builder pattern for testing
#[cfg(test)]
#[derive(Default)]
pub struct PackageNodeBuilder {
    name: String,
    version: String,
    environments: HashMap<String, EnvironmentConfig>,
}

#[cfg(test)]
impl PackageNodeBuilder {
    pub fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub fn version(mut self, version: &str) -> Self {
        self.version = version.to_string();
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
            },
        );
        self
    }

    pub fn build(self) -> PackageNode {
        PackageNode::new(self.name, self.version, self.environments)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_package_node() {
        let mut environments = HashMap::new();
        environments.insert(
            "test-env".to_string(),
            EnvironmentConfig {
                install: "test install".to_string(),
            },
        );

        let package = PackageNode::new(
            "test-package".to_string(),
            "1.0.0".to_string(),
            environments,
        );

        assert_eq!(package.name, "test-package");
        assert_eq!(package.version, "1.0.0");
        assert_eq!(package.environments.len(), 1);
        assert_eq!(
            package.environments.get("test-env").unwrap().install,
            "test install"
        );
    }

    #[test]
    fn test_validate_valid_package() {
        let package = PackageNodeBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        assert!(package.validate().is_ok());
    }

    #[test]
    fn test_validate_missing_fields() {
        let package = PackageNodeBuilder::default().build();

        assert_eq!(
            package.validate(),
            Err(PackageValidationError::EmptyField("name".to_string()))
        );
    }

    #[test]
    fn test_validate_empty_fields() {
        let package = PackageNodeBuilder::default()
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
        let package = PackageNodeBuilder::default()
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
}
