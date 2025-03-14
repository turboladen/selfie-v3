// src/services/package/validate.rs
use std::path::Path;

use thiserror::Error;

use crate::{
    domain::{
        config::AppConfig,
        package::{Package, PackageParseError},
        validation::{ValidationErrorCategory, ValidationIssue, ValidationResult},
    },
    ports::{
        command::CommandRunner,
        filesystem::{FileSystem, FileSystemError},
        package_repo::{PackageRepoError, PackageRepository},
    },
    services::command_validator::CommandValidator,
};

#[derive(Error, Debug)]
pub(crate) enum PackageValidatorError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Multiple packages found with name: {0}")]
    MultiplePackagesFound(String),

    #[error("Invalid package: {0}")]
    InvalidPackage(String),

    #[error("Package repository error: {0}")]
    RepoError(#[from] PackageRepoError),

    #[error("File system error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("Package parse error: {0}")]
    ParseError(#[from] PackageParseError),

    #[error("Command execution error: {0}")]
    CommandError(String),
}

/// Validates package files with detailed error reporting and command validation
pub(crate) struct PackageValidator<'a> {
    fs: &'a dyn FileSystem,
    runner: &'a dyn CommandRunner,
    config: &'a AppConfig,
    package_repo: &'a dyn PackageRepository,
    command_validator: CommandValidator<'a>, // Add CommandValidator
}

impl<'a> PackageValidator<'a> {
    /// Create a new package validator
    pub(crate) fn new(
        fs: &'a dyn FileSystem,
        runner: &'a dyn CommandRunner,
        config: &'a AppConfig,
        package_repo: &'a dyn PackageRepository,
    ) -> Self {
        // Create a CommandValidator instance
        let command_validator = CommandValidator::new(runner);

        Self {
            fs,
            runner,
            config,
            package_repo,
            command_validator,
        }
    }

    /// Validate a package by name
    pub(crate) async fn validate_package_by_name(
        &self,
        package_name: &str,
    ) -> Result<ValidationResult, PackageValidatorError> {
        // Find the package file using the repository
        let package_files = self
            .package_repo
            .find_package_files(package_name)
            .map_err(PackageValidatorError::RepoError)?;

        if package_files.is_empty() {
            return Err(PackageValidatorError::PackageNotFound(
                package_name.to_string(),
            ));
        }

        if package_files.len() > 1 {
            return Err(PackageValidatorError::MultiplePackagesFound(
                package_name.to_string(),
            ));
        }

        let package_path = &package_files[0];
        self.validate_package_file(package_path).await
    }

    /// Validate a specific package file
    pub(crate) async fn validate_package_file(
        &self,
        package_path: &Path,
    ) -> Result<ValidationResult, PackageValidatorError> {
        // Read and parse the package file
        let file_content = self
            .fs
            .read_file(package_path)
            .map_err(PackageValidatorError::FileSystemError)?;

        // Try to parse the package, but continue even if it fails
        let package = Package::from_yaml(&file_content);

        // Get the package name either from the parsed package or the file name
        let package_name = match &package {
            Ok(pkg) => pkg.name.clone(),
            Err(_) => package_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
        };

        let mut result = ValidationResult::new(&package_name).with_path(package_path.to_path_buf());

        // If parsing failed, add the parse error and return early
        match package {
            Ok(pkg) => {
                // Start with domain validation
                let domain_issues = pkg.validate(self.config.environment());
                result.add_issues(domain_issues);

                // Run the enhanced validation which now includes command validation
                self.enhance_validation(&pkg, &mut result).await;

                // Set the package
                result = result.with_package(pkg);
            }
            Err(err) => {
                // Add the parse error
                result.add_issue(ValidationIssue::error(
                    ValidationErrorCategory::Other,
                    "package",
                    &format!("Failed to parse package file: {}", err),
                    None,
                    Some("Check the YAML format and fix the syntax errors."),
                ));
            }
        }

        Ok(result)
    }

    /// Enhanced validation that includes command validation
    async fn enhance_validation(&self, package: &Package, result: &mut ValidationResult) {
        // Add command availability checks
        self.validate_command_availability(package, result).await;

        // Add command syntax validation
        self.validate_command_syntax(package, result);

        // Add environment-specific recommendations
        self.validate_environment_recommendations(package, result);
    }

    /// Validate command availability
    async fn validate_command_availability(
        &self,
        package: &Package,
        result: &mut ValidationResult,
    ) {
        // We only check commands for the current environment
        if let Some(env_config) = package.environments.get(self.config.environment()) {
            // Extract base command from install command
            if let Some(base_cmd) = CommandValidator::extract_base_command(&env_config.install) {
                let availability_result = self
                    .command_validator
                    .check_command_availability(self.config.environment(), base_cmd)
                    .await;

                if !availability_result.is_available {
                    result.add_issue(ValidationIssue::warning(
                        ValidationErrorCategory::Availability,
                        &format!("environments.{}.install", self.config.environment()),
                        &availability_result.error.unwrap_or_default(),
                        None,
                        Some("Install the command before using this package."),
                    ));
                }
            }

            // Check command if present
            if let Some(check_cmd) = &env_config.check {
                if let Some(base_cmd) = CommandValidator::extract_base_command(check_cmd) {
                    let availability_result = self
                        .command_validator
                        .check_command_availability(self.config.environment(), base_cmd)
                        .await;

                    if !availability_result.is_available {
                        result.add_issue(ValidationIssue::warning(
                            ValidationErrorCategory::Availability,
                            &format!("environments.{}.check", self.config.environment()),
                            &availability_result.error.unwrap_or_default(),
                            None,
                            Some("Install the command before using this package."),
                        ));
                    }
                }
            }
        }
    }

    /// Validate command syntax
    fn validate_command_syntax(&self, package: &Package, result: &mut ValidationResult) {
        for (env_name, env_config) in &package.environments {
            // Validate install command
            let install_field = &format!("environments.{}.install", env_name);
            let install_validation = self
                .command_validator
                .validate_command_syntax(env_name, &env_config.install);

            if !install_validation.is_valid {
                result.add_issue(ValidationIssue::error(
                    ValidationErrorCategory::CommandSyntax,
                    install_field,
                    &install_validation.error.unwrap_or_default(),
                    None,
                    Some("Check the command syntax and fix any issues."),
                ));
            }

            // Validate check command if present
            if let Some(check_cmd) = &env_config.check {
                let check_field = &format!("environments.{}.check", env_name);
                let check_validation = self
                    .command_validator
                    .validate_command_syntax(env_name, check_cmd);

                if !check_validation.is_valid {
                    result.add_issue(ValidationIssue::error(
                        ValidationErrorCategory::CommandSyntax,
                        check_field,
                        &check_validation.error.unwrap_or_default(),
                        None,
                        Some("Check the command syntax and fix any issues."),
                    ));
                }
            }

            // Add warnings for potential issues detected by CommandValidator
            if self
                .command_validator
                .might_require_sudo(&env_config.install)
            {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", env_name),
                    "Command might require sudo privileges",
                    None,
                    Some("This command may require administrative privileges to run."),
                ));
            }

            if self.command_validator.uses_backticks(&env_config.install) {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", env_name),
                    "Command uses backticks for command substitution",
                    None,
                    Some("Consider using $() for command substitution instead of backticks."),
                ));
            }

            if self
                .command_validator
                .might_download_content(&env_config.install)
            {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", env_name),
                    "Command may download content from the internet",
                    None,
                    Some(
                        "This command appears to download content, which may pose security risks.",
                    ),
                ));
            }
        }
    }

    /// Add environment-specific recommendations
    fn validate_environment_recommendations(
        &self,
        package: &Package,
        result: &mut ValidationResult,
    ) {
        // We only check for the current environment
        if let Some(env_config) = package.environments.get(self.config.environment()) {
            if let Some(recommendation) = self
                .command_validator
                .is_command_recommended_for_env(self.config.environment(), &env_config.install)
            {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::Environment,
                    &format!("environments.{}.install", self.config.environment()),
                    &recommendation,
                    None,
                    Some("Using environment-specific package managers may improve reliability."),
                ));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::path::Path;

    use crate::{
        adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
        domain::config::AppConfigBuilder,
        ports::{
            command::MockCommandRunner, filesystem::MockFileSystem,
            package_repo::MockPackageRepository,
        },
    };

    // Helper function to create a test environment
    fn setup_test_environment() -> (MockFileSystem, MockCommandRunner, AppConfig) {
        let mut fs = MockFileSystem::default();
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        // Add the package directory to the filesystem
        fs.mock_path_exists("/test/packages", true);

        let runner = MockCommandRunner::new();

        (fs, runner, config)
    }

    // Helper to create a valid package YAML
    fn create_valid_package_yaml() -> String {
        r#"
name: test-package
version: 1.0.0
homepage: https://example.com
description: A test package
environments:
  test-env:
    install: brew install test-package
    check: which test-package
  prod-env:
    install: apt-get install test-package
    dependencies:
      - dependency1
      - dependency2
"#
        .to_string()
    }

    #[tokio::test]
    async fn test_validate_valid_package() {
        let (mut fs, mut runner, config) = setup_test_environment();

        // Add a valid package file
        let yaml = create_valid_package_yaml();

        fs.mock_path_exists(Path::new("/test/packages/test-package.yaml"), true);
        fs.mock_path_exists(Path::new("/test/packages/test-package.yml"), false);
        fs.mock_read_file(Path::new("/test/packages/test-package.yaml"), &yaml);

        runner.mock_is_command_available("brew", true);
        runner.mock_is_command_available("which", true);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator
            .validate_package_by_name("test-package")
            .await
            .unwrap();

        assert!(result.is_valid());
        assert_eq!(result.issues.len(), 1);
        assert_eq!(
            result.issues[0].category,
            ValidationErrorCategory::CommandSyntax
        );
    }

    #[tokio::test]
    async fn test_validate_missing_required_fields() {
        let (mut fs, mut runner, config) = setup_test_environment();

        // Add an invalid package file with missing fields
        // Using valid YAML with empty fields rather than missing fields
        let yaml = r#"
name: ""
version: ""
environments:
  test-env:
    install: brew install test-package
"#;
        fs.mock_path_exists("/test/packages/incomplete.yaml", true);
        fs.mock_read_file("/test/packages/incomplete.yaml", yaml);

        runner.mock_is_command_available("brew", true);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator
            .validate_package_file(Path::new("/test/packages/incomplete.yaml"))
            .await
            .unwrap();

        assert!(!result.is_valid());

        // Should have errors for missing name and version
        let required_field_errors =
            result.issues_by_category(&ValidationErrorCategory::RequiredField);
        assert_eq!(required_field_errors.len(), 2);

        // Check specific error messages
        let name_error = required_field_errors.iter().find(|e| e.field == "name");
        assert!(name_error.is_some());

        let version_error = required_field_errors.iter().find(|e| e.field == "version");
        assert!(version_error.is_some());
    }

    #[tokio::test]
    async fn test_validate_invalid_url() {
        let (mut fs, mut runner, config) = setup_test_environment();

        // Add a package with invalid URL
        let yaml = r#"
name: test-package
version: 1.0.0
homepage: not-a-valid-url
environments:
  test-env:
    install: brew install test-package
"#;
        fs.mock_read_file("/test/packages/invalid-url.yaml", yaml);

        runner.mock_is_command_available("brew", true);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator
            .validate_package_file(Path::new("/test/packages/invalid-url.yaml"))
            .await
            .unwrap();

        // Should have a URL format error
        let url_errors = result.issues_by_category(&ValidationErrorCategory::UrlFormat);
        assert_eq!(url_errors.len(), 1);
        assert_eq!(url_errors[0].field, "homepage");
    }

    #[tokio::test]
    async fn test_validate_command_syntax() {
        let (mut fs, mut runner, config) = setup_test_environment();

        // Add a package with command syntax errors
        let yaml = r#"
name: test-package
version: 1.0.0
environments:
  test-env:
    install: brew install test-package "with unmatched quote
    check: echo "hello | | invalid pipes"
"#;
        fs.mock_read_file("/test/packages/bad-commands.yaml", yaml);

        runner.mock_is_command_available("brew", true);
        runner.mock_is_command_available("echo", true);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator
            .validate_package_file(Path::new("/test/packages/bad-commands.yaml"))
            .await
            .unwrap();

        // Should have command syntax errors
        let cmd_errors = result.issues_by_category(&ValidationErrorCategory::CommandSyntax);
        assert!(cmd_errors.len() >= 2); // At least the quote and pipe errors

        // Check specific error types
        assert!(cmd_errors
            .iter()
            .any(|e| e.message.contains("Unmatched double quote")));
        assert!(cmd_errors
            .iter()
            .any(|e| e.message.contains("Invalid pipe usage")));
    }

    // Rest of tests...
}
