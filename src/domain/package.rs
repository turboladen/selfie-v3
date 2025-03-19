// Core package entity and related types
use std::{
    collections::HashMap,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};
use thiserror::Error;

use crate::{
    domain::validation::{ValidationErrorCategory, ValidationIssue},
    ports::filesystem::FileSystem,
};

/// Core package entity representing a package definition
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct Package {
    /// Package name
    pub(crate) name: String,

    /// Package version (for the selfie package file, not the underlying package)
    pub(crate) version: String,

    /// Optional homepage URL
    #[serde(default)]
    pub(crate) homepage: Option<String>,

    /// Optional package description
    #[serde(default)]
    pub(crate) description: Option<String>,

    /// Map of environment configurations
    #[serde(default)]
    pub(crate) environments: HashMap<String, EnvironmentConfig>,

    /// Path to the package file (not serialized/deserialized)
    #[serde(skip)]
    pub(crate) path: PathBuf,
}

/// Configuration for a specific environment
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub(crate) struct EnvironmentConfig {
    /// Command to install the package
    pub(crate) install: String,

    /// Optional command to check if the package is already installed
    #[serde(default)]
    pub(crate) check: Option<String>,

    /// Dependencies that must be installed before this package
    #[serde(default)]
    pub(crate) dependencies: Vec<String>,
}

/// Errors related to package validation
#[derive(Error, Debug, PartialEq)]
pub(crate) enum PackageValidationError {
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
pub(crate) enum PackageParseError {
    #[error("YAML parsing error: {0}")]
    YamlParse(#[from] serde_yaml::Error),

    #[error("I/O error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("File system error: {0}")]
    FileSystemError(String),
}

impl Package {
    /// Create a new package with the specified attributes
    #[cfg(test)]
    pub(crate) fn new(
        name: String,
        version: String,
        homepage: Option<String>,
        description: Option<String>,
        environments: HashMap<String, EnvironmentConfig>,
        path: PathBuf,
    ) -> Self {
        Self {
            name,
            version,
            homepage,
            description,
            environments,
            path,
        }
    }

    /// Resolve an environment configuration by name
    pub(crate) fn resolve_environment(
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

    pub(crate) fn from_yaml(yaml_str: &str) -> Result<Self, PackageParseError> {
        let mut package: Self = serde_yaml::from_str(yaml_str)?;

        // Ensure defaults are set
        for env_config in package.environments.values_mut() {
            if env_config.dependencies.is_empty() {
                env_config.dependencies = Vec::new();
            }
        }

        Ok(package)
    }

    // Load a Package from a file using the FileSystem trait
    pub(crate) fn from_file<F: FileSystem>(fs: &F, path: &Path) -> Result<Self, PackageParseError> {
        let content = fs
            .read_file(path)
            .map_err(|e| PackageParseError::FileSystemError(e.to_string()))?;

        let mut package = Self::from_yaml(&content)?;
        package.path = path.to_path_buf();

        Ok(package)
    }

    // Serialize to YAML
    #[cfg(test)]
    pub(crate) fn to_yaml(&self) -> Result<String, serde_yaml::Error> {
        serde_yaml::to_string(self)
    }

    /// Validate required fields for the package
    pub(crate) fn validate_required_fields(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check name
        if self.name.is_empty() {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                "name",
                "Package name is required",
                None,
                Some("Add 'name: your-package-name' to the package file."),
            ));
        } else if !Self::is_valid_package_name(&self.name) {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::InvalidValue,
                "name",
                "Package name contains invalid characters",
                None,
                Some("Use only alphanumeric characters, hyphens, and underscores."),
            ));
        }

        // Check version
        if self.version.is_empty() {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                "version",
                "Package version is required",
                None,
                Some("Add 'version: \"0.1.0\"' to the package file."),
            ));
        } else if !Self::is_valid_version(&self.version) {
            issues.push(ValidationIssue::warning(
                ValidationErrorCategory::InvalidValue,
                "version",
                "Package version should follow semantic versioning",
                None,
                Some("Consider using a semantic version like '1.0.0'."),
            ));
        }

        // Check environments
        if self.environments.is_empty() {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                "environments",
                "At least one environment must be defined",
                None,
                Some("Add an 'environments' section with at least one environment."),
            ));
        }

        issues
    }

    /// Validate URL fields
    pub(crate) fn validate_urls(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check homepage URL if present
        if let Some(homepage) = &self.homepage {
            match url::Url::parse(homepage) {
                Ok(url) => {
                    // Check scheme
                    if url.scheme() != "http" && url.scheme() != "https" {
                        issues.push(ValidationIssue::warning(
                            ValidationErrorCategory::UrlFormat,
                            "homepage",
                            &format!(
                                "URL should use http or https scheme, found: {}",
                                url.scheme()
                            ),
                            None,
                            Some("Use https:// prefix for the URL."),
                        ));
                    }
                }
                Err(err) => {
                    issues.push(ValidationIssue::error(
                        ValidationErrorCategory::UrlFormat,
                        "homepage",
                        &format!("Invalid URL format: {}", err),
                        None,
                        Some("Provide a valid URL with http:// or https:// prefix."),
                    ));
                }
            }
        }

        issues
    }

    /// Validate environments configuration
    pub(crate) fn validate_environments(&self, current_env: &str) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Already checked if environments is empty in validate_required_fields
        if self.environments.is_empty() {
            return issues;
        }

        // Check if current environment is configured
        if !current_env.is_empty() && !self.environments.contains_key(current_env) {
            issues.push(ValidationIssue::warning(
                ValidationErrorCategory::Environment,
                "environments",
                &format!("Current environment '{}' is not configured", current_env),
                None,
                Some(&format!(
                    "Add an environment section for '{}' if needed for this environment.",
                    current_env
                )),
            ));
        }

        // Validate each environment's required fields
        for (env_name, env_config) in &self.environments {
            if env_config.install.is_empty() {
                issues.push(ValidationIssue::error(
                    ValidationErrorCategory::RequiredField,
                    &format!("environments.{}.install", env_name),
                    "Install command is required",
                    None,
                    Some("Add an install command like 'brew install package-name'."),
                ));
            }

            // Validate dependencies (check for empty names)
            for (i, dep) in env_config.dependencies.iter().enumerate() {
                if dep.is_empty() {
                    issues.push(ValidationIssue::error(
                        ValidationErrorCategory::InvalidValue,
                        &format!("environments.{}.dependencies[{}]", env_name, i),
                        "Dependency name cannot be empty",
                        None,
                        Some("Remove the empty dependency or provide a valid name."),
                    ));
                }
            }
        }

        issues
    }

    /// Basic command syntax validation that doesn't require external dependencies
    pub(crate) fn validate_command_syntax(&self) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        for (env_name, env_config) in &self.environments {
            // Check install command syntax
            issues.extend(Self::validate_single_command(
                &env_config.install,
                &format!("environments.{}.install", env_name),
            ));

            // Check check command syntax if present
            if let Some(check_cmd) = &env_config.check {
                issues.extend(Self::validate_single_command(
                    check_cmd,
                    &format!("environments.{}.check", env_name),
                ));
            }
        }

        issues
    }

    /// Validate a single command for syntax issues
    fn validate_single_command(command: &str, field_name: &str) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        // Check for unmatched quotes
        let mut in_single_quotes = false;
        let mut in_double_quotes = false;

        for c in command.chars() {
            match c {
                '\'' if !in_double_quotes => in_single_quotes = !in_single_quotes,
                '"' if !in_single_quotes => in_double_quotes = !in_double_quotes,
                _ => {}
            }
        }

        if in_single_quotes {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Unmatched single quote in command",
                None,
                Some("Add a closing single quote (') to the command."),
            ));
        }

        if in_double_quotes {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Unmatched double quote in command",
                None,
                Some("Add a closing double quote (\") to the command."),
            ));
        }

        // Check for invalid pipe usage
        if command.contains("| |") {
            issues.push(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Invalid pipe usage in command",
                None,
                Some("Remove duplicate pipe symbols."),
            ));
        }

        // Check for invalid redirections
        for redirect in &[">", ">>", "<"] {
            if command.contains(&format!("{} ", redirect))
                && !command.contains(&format!("{} /", redirect))
                && !command.contains(&format!("{} ~/", redirect))
            {
                issues.push(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    field_name,
                    &format!("Potential invalid redirection with {}", redirect),
                    None,
                    Some("Ensure the redirection path is valid and absolute."),
                ));
            }
        }

        // Check for command injection risks with backticks
        if command.contains('`') {
            issues.push(ValidationIssue::warning(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Contains command substitution with backticks",
                None,
                Some("Consider using $() for command substitution instead of backticks."),
            ));
        }

        issues
    }

    /// Check if a string is a valid package name
    fn is_valid_package_name(name: &str) -> bool {
        // Package names should only contain alphanumeric chars, hyphens, and underscores
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// Check if a string is a valid version (basic semantic versioning check)
    fn is_valid_version(version: &str) -> bool {
        // Simple check for semver format: major.minor.patch
        let semver_regex = regex::Regex::new(r"^\d+\.\d+\.\d+").unwrap();
        semver_regex.is_match(version)
    }

    /// Perform all basic domain validations
    pub(crate) fn validate(&self, current_env: &str) -> Vec<ValidationIssue> {
        let mut issues = Vec::new();

        issues.extend(self.validate_required_fields());
        issues.extend(self.validate_urls());
        issues.extend(self.validate_environments(current_env));
        issues.extend(self.validate_command_syntax());

        issues
    }
}

// Builder pattern for testing
#[cfg(test)]
#[derive(Default)]
pub(crate) struct PackageBuilder {
    name: String,
    version: String,
    homepage: Option<String>,
    description: Option<String>,
    environments: HashMap<String, EnvironmentConfig>,
    path: PathBuf,
}

#[cfg(test)]
impl PackageBuilder {
    pub(crate) fn name(mut self, name: &str) -> Self {
        self.name = name.to_string();
        self
    }

    pub(crate) fn version(mut self, version: &str) -> Self {
        self.version = version.to_string();
        self
    }

    pub(crate) fn homepage(mut self, homepage: &str) -> Self {
        self.homepage = Some(homepage.to_string());
        self
    }

    pub(crate) fn description(mut self, description: &str) -> Self {
        self.description = Some(description.to_string());
        self
    }

    pub(crate) fn environment<T>(mut self, name: T, install_command: &str) -> Self
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

    pub(crate) fn environment_with_check<T>(
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

    pub(crate) fn environment_with_dependencies<T>(
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

    pub(crate) fn build(self) -> Package {
        Package::new(
            self.name,
            self.version,
            self.homepage,
            self.description,
            self.environments,
            self.path,
        )
    }
}

#[cfg(test)]
mod tests {
    use crate::ports::filesystem::{FileSystemError, MockFileSystem};

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

        assert!(package.validate("test-env").is_empty());
    }

    #[test]
    fn test_validate_missing_fields() {
        let package = PackageBuilder::default().build();

        pretty_assertions::assert_eq!(
            package.validate("test-env"),
            vec![
                ValidationIssue {
                    category: ValidationErrorCategory::RequiredField,
                    field: "name".to_string(),
                    message: "Package name is required".to_string(),
                    line: None,
                    is_warning: false,
                    suggestion: Some(
                        "Add 'name: your-package-name' to the package file.".to_string()
                    )
                },
                ValidationIssue {
                    category: ValidationErrorCategory::RequiredField,
                    field: "version".to_string(),
                    message: "Package version is required".to_string(),
                    line: None,
                    is_warning: false,
                    suggestion: Some("Add 'version: \"0.1.0\"' to the package file.".to_string())
                },
                ValidationIssue {
                    category: ValidationErrorCategory::RequiredField,
                    field: "environments".to_string(),
                    message: "At least one environment must be defined".to_string(),
                    line: None,
                    is_warning: false,
                    suggestion: Some(
                        "Add an 'environments' section with at least one environment.".to_string()
                    )
                }
            ]
        );
    }

    #[test]
    fn test_validate_empty_fields() {
        let package = PackageBuilder::default()
            .name("")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        pretty_assertions::assert_eq!(
            package.validate("test-env"),
            vec![ValidationIssue {
                category: ValidationErrorCategory::RequiredField,
                field: "name".to_string(),
                message: "Package name is required".to_string(),
                line: None,
                is_warning: false,
                suggestion: Some("Add 'name: your-package-name' to the package file.".to_string())
            },]
        );
    }

    #[test]
    fn test_validate_empty_environment() {
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "")
            .build();

        pretty_assertions::assert_eq!(
            package.validate("test-env"),
            vec![ValidationIssue {
                category: ValidationErrorCategory::RequiredField,
                field: "environments.test-env.install".to_string(),
                message: "Install command is required".to_string(),
                line: None,
                is_warning: false,
                suggestion: Some(
                    "Add an install command like 'brew install package-name'.".to_string()
                )
            },]
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

        let package = Package::from_yaml(yaml).unwrap();

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
        let package = PackageBuilder::default()
            .name("ripgrep")
            .version("0.1.0")
            .environment_with_check("mac", "brew install ripgrep", "which rg")
            .environment_with_dependencies("linux", "apt install ripgrep", vec!["apt"])
            .build();

        let yaml = package.to_yaml().unwrap();
        let parsed_package = Package::from_yaml(&yaml).unwrap();

        assert_eq!(package.name, parsed_package.name);
        assert_eq!(package.version, parsed_package.version);
        assert_eq!(
            package.environments.len(),
            parsed_package.environments.len()
        );
    }

    #[test]
    fn test_package_from_file() {
        let mut fs = MockFileSystem::default();
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

        fs.mock_read_file(path, yaml);

        let package = Package::from_file(&fs, path).unwrap();

        assert_eq!(package.name, "ripgrep");
        assert_eq!(package.version, "0.1.0");
        assert_eq!(package.environments.len(), 2);
        assert_eq!(package.path, path.to_path_buf());
    }

    #[test]
    fn test_package_from_file_not_found() {
        let path = Path::new("/test/packages/nonexistent.yaml");

        let mut fs = MockFileSystem::default();
        fs.expect_read_file()
            .with(mockall::predicate::eq(path))
            .returning(move |_| Err(FileSystemError::PathNotFound("meow".to_string())));

        let result = Package::from_file(&fs, path);
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

        let result = Package::from_yaml(yaml);
        assert!(result.is_err());
    }

    // Tests to add to the Package module tests

    #[test]
    fn test_validate_required_fields() {
        let empty_package = Package::new(
            String::new(),
            String::new(),
            None,
            None,
            HashMap::new(),
            PathBuf::new(),
        );

        let issues = empty_package.validate_required_fields();

        // Should have errors for name, version, and environments
        assert_eq!(issues.len(), 3);
        assert!(issues.iter().any(|i| i.field == "name" && !i.is_warning));
        assert!(issues.iter().any(|i| i.field == "version" && !i.is_warning));
        assert!(issues
            .iter()
            .any(|i| i.field == "environments" && !i.is_warning));
    }

    #[test]
    fn test_validate_urls() {
        // Test invalid URL
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .homepage("not-a-valid-url")
            .environment("test-env", "test install")
            .build();

        let issues = package.validate_urls();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].category == ValidationErrorCategory::UrlFormat);

        // Test valid URL but wrong scheme (ftp)
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .homepage("ftp://example.com")
            .environment("test-env", "test install")
            .build();

        let issues = package.validate_urls();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_warning);
        assert!(issues[0].message.contains("scheme"));

        // Test valid URL with correct scheme
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .homepage("https://example.com")
            .environment("test-env", "test install")
            .build();

        let issues = package.validate_urls();
        assert_eq!(issues.len(), 0);
    }

    #[test]
    fn test_validate_environments() {
        // Test missing current environment
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("other-env", "test install")
            .build();

        let issues = package.validate_environments("test-env");
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_warning);
        assert!(issues[0].message.contains("not configured"));

        // Test empty install command
        let mut package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .build();

        let env_config = EnvironmentConfig {
            install: String::new(),
            check: None,
            dependencies: vec![],
        };

        package
            .environments
            .insert("test-env".to_string(), env_config);

        let issues = package.validate_environments("test-env");
        assert_eq!(issues.len(), 1);
        assert!(!issues[0].is_warning); // This should be an error
        assert!(issues[0].message.contains("required"));
    }

    #[test]
    fn test_validate_command_syntax() {
        // Test unmatched quote
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "echo 'unmatched")
            .build();

        let issues = package.validate_command_syntax();
        assert_eq!(issues.len(), 1);
        assert!(!issues[0].is_warning);
        assert!(issues[0].message.contains("Unmatched single quote"));

        // Test invalid pipe
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "echo test | | grep test")
            .build();

        let issues = package.validate_command_syntax();
        assert_eq!(issues.len(), 1);
        assert!(!issues[0].is_warning);
        assert!(issues[0].message.contains("Invalid pipe usage"));

        // Test backticks (warning)
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "echo `date`")
            .build();

        let issues = package.validate_command_syntax();
        assert_eq!(issues.len(), 1);
        assert!(issues[0].is_warning);
        assert!(issues[0].message.contains("backticks"));
    }

    #[test]
    fn test_full_validate() {
        // Test a valid package
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .homepage("https://example.com")
            .description("A test package")
            .environment("test-env", "echo test")
            .build();

        let issues = package.validate("test-env");
        assert_eq!(issues.len(), 0);

        // Test an invalid package with multiple issues
        let package = PackageBuilder::default()
            .name("")
            .version("")
            .homepage("invalid-url")
            .environment("other-env", "echo `test`")
            .build();

        let issues = package.validate("test-env");
        assert!(issues.len() >= 4); // At least 4 issues should be found
    }
}
