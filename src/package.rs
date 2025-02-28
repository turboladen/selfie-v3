// src/package.rs

use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PackageNode {
    pub name: String,
    pub version: String,
    #[serde(default)]
    pub homepage: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub environments: HashMap<String, EnvironmentConfig>,
    #[serde(skip, default)]
    pub path: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EnvironmentConfig {
    pub install: String,
    #[serde(default)]
    pub check: Option<String>,
    #[serde(default)]
    pub dependencies: Vec<String>,
}

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

#[derive(Error, Debug)]
pub enum PackageParseError {
    #[error("YAML parsing error: {0}")]
    YamlError(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("File system error: {0}")]
    FileSystemError(String),
}

impl PackageNode {
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

    // Set the file path this package was loaded from
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.path = Some(path);
        self
    }

    // Parse a PackageNode from YAML string
    pub fn from_yaml(yaml_str: &str) -> Result<Self, PackageParseError> {
        let mut package: PackageNode = serde_yaml::from_str(yaml_str)?;

        // Ensure defaults are set
        for env_config in package.environments.values_mut() {
            if env_config.dependencies.is_empty() {
                env_config.dependencies = Vec::new();
            }
        }

        Ok(package)
    }

    // Load a PackageNode from a file using the FileSystem trait
    pub async fn from_file<F: crate::filesystem::FileSystem>(
        fs: &F,
        path: &Path,
    ) -> Result<Self, PackageParseError> {
        let content = fs
            .read_file(path)
            .await
            .map_err(|e| PackageParseError::FileSystemError(e.to_string()))?;

        let mut package = Self::from_yaml(&content)?;
        package.path = Some(path.to_path_buf());

        Ok(package)
    }

    // Serialize to YAML
    pub fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
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

    pub fn resolve_environment(
        &self,
        config_env: &str,
    ) -> Result<&EnvironmentConfig, PackageValidationError> {
        self.environments.get(config_env).ok_or_else(|| {
            if self.environments.is_empty() {
                PackageValidationError::MissingField("environments".to_string())
            } else {
                PackageValidationError::EnvironmentNotSupported(config_env.to_string())
            }
        })
    }
}

// Builder pattern for testing
#[cfg(test)]
#[derive(Default)]
pub struct PackageNodeBuilder {
    name: String,
    version: String,
    homepage: Option<String>,
    description: Option<String>,
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

    pub fn build(self) -> PackageNode {
        PackageNode::new(
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
    use crate::filesystem::mock::MockFileSystem;
    use std::path::Path;

    #[test]
    fn test_create_package_node() {
        let mut environments = HashMap::new();
        environments.insert(
            "test-env".to_string(),
            EnvironmentConfig {
                install: "test install".to_string(),
                check: None,
                dependencies: Vec::new(),
            },
        );

        let package = PackageNode::new(
            "test-package".to_string(),
            "1.0.0".to_string(),
            None,
            None,
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
    fn test_create_package_with_metadata() {
        let mut environments = HashMap::new();
        environments.insert(
            "test-env".to_string(),
            EnvironmentConfig {
                install: "test install".to_string(),
                check: None,
                dependencies: Vec::new(),
            },
        );

        let package = PackageNode::new(
            "test-package".to_string(),
            "1.0.0".to_string(),
            Some("https://example.com".to_string()),
            Some("Test package description".to_string()),
            environments,
        );

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
    fn test_package_builder_with_metadata() {
        let package = PackageNodeBuilder::default()
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

    #[test]
    fn test_resolve_environment_valid() {
        let package = PackageNodeBuilder::default()
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
        let package = PackageNodeBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("Test-Env", "test install")
            .build();

        let result = package.resolve_environment("test-env");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_environment_not_found() {
        let package = PackageNodeBuilder::default()
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
        let package = PackageNodeBuilder::default()
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

    #[test]
    fn test_package_from_yaml() {
        let yaml = r#"
            name: ripgrep
            version: 0.1.0
            homepage: https://example.com
            description: Fast line-oriented search tool
            environments:
              mac:
                install: brew install ripgrep
                check: which rg
                dependencies:
                  - brew
              linux:
                install: apt install ripgrep
        "#;

        let package = PackageNode::from_yaml(yaml).unwrap();

        assert_eq!(package.name, "ripgrep");
        assert_eq!(package.version, "0.1.0");
        assert_eq!(package.homepage, Some("https://example.com".to_string()));
        assert_eq!(
            package.description,
            Some("Fast line-oriented search tool".to_string())
        );
        assert_eq!(package.environments.len(), 2);
        assert_eq!(
            package.environments.get("mac").unwrap().install,
            "brew install ripgrep"
        );
        assert_eq!(
            package.environments.get("mac").unwrap().check,
            Some("which rg".to_string())
        );
        assert_eq!(
            package.environments.get("mac").unwrap().dependencies,
            vec!["brew"]
        );
        assert_eq!(
            package.environments.get("linux").unwrap().install,
            "apt install ripgrep"
        );
        assert_eq!(package.environments.get("linux").unwrap().check, None);
        assert!(package
            .environments
            .get("linux")
            .unwrap()
            .dependencies
            .is_empty());
    }

    #[test]
    fn test_package_to_yaml() {
        let package = PackageNodeBuilder::default()
            .name("ripgrep")
            .version("0.1.0")
            .environment_with_check("mac", "brew install ripgrep", "which rg")
            .environment_with_dependencies("linux", "apt install ripgrep", vec!["apt"])
            .build();

        let yaml = package.to_yaml().unwrap();
        let parsed_package = PackageNode::from_yaml(&yaml).unwrap();

        assert_eq!(package.name, parsed_package.name);
        assert_eq!(package.version, parsed_package.version);
        assert_eq!(
            package.environments.len(),
            parsed_package.environments.len()
        );
    }

    #[tokio::test]
    async fn test_package_from_file() {
        let fs = MockFileSystem::default();
        let path = Path::new("/test/packages/ripgrep.yaml");

        let yaml = r#"
            name: ripgrep
            version: 0.1.0
            environments:
              mac:
                install: brew install ripgrep
                check: which rg
                dependencies:
                  - brew
              linux:
                install: apt install ripgrep
        "#;

        fs.add_file(path, yaml);

        let package = PackageNode::from_file(&fs, path).await.unwrap();

        assert_eq!(package.name, "ripgrep");
        assert_eq!(package.version, "0.1.0");
        assert_eq!(package.environments.len(), 2);
        assert_eq!(package.path, Some(path.to_path_buf()));
    }

    #[tokio::test]
    async fn test_package_from_file_not_found() {
        let fs = MockFileSystem::default();
        let path = Path::new("/test/packages/nonexistent.yaml");

        let result = PackageNode::from_file(&fs, path).await;
        assert!(result.is_err());
    }

    #[test]
    fn test_package_from_invalid_yaml() {
        let yaml = r#"
            name: ripgrep
            version: 0.1.0
            environments:
              - this is invalid YAML for our structure
        "#;

        let result = PackageNode::from_yaml(yaml);
        assert!(result.is_err());
    }
}
