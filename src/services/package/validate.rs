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
}

impl<'a> PackageValidator<'a> {
    /// Create a new package validator
    pub(crate) fn new(
        fs: &'a dyn FileSystem,
        runner: &'a dyn CommandRunner,
        config: &'a AppConfig,
        package_repo: &'a dyn PackageRepository,
    ) -> Self {
        Self {
            fs,
            runner,
            config,
            package_repo,
        }
    }

    /// Validate a package by name
    pub(crate) fn validate_package_by_name(
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
        self.validate_package_file(package_path)
    }

    /// Validate a specific package file
    pub(crate) fn validate_package_file(
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
                self.enhance_validation(&pkg, &mut result);

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
            if let Some(base_cmd) = Self::extract_base_command(&env_config.install) {
                let is_available = self.runner.is_command_available(base_cmd).await;

                if !is_available {
                    result.add_issue(ValidationIssue::warning(
                        ValidationErrorCategory::Availability,
                        &format!("environments.{}.install", self.config.environment()),
                        &format!(
                            "Command '{}' not found in environment '{}'",
                            base_cmd,
                            self.config.environment()
                        ),
                        None,
                        Some("Install the command before using this package."),
                    ));
                }
            }

            // Check command if present
            if let Some(check_cmd) = &env_config.check {
                if let Some(base_cmd) = Self::extract_base_command(check_cmd) {
                    let is_available = self.runner.is_command_available(base_cmd).await;

                    if !is_available {
                        result.add_issue(ValidationIssue::warning(
                            ValidationErrorCategory::Availability,
                            &format!("environments.{}.check", self.config.environment()),
                            &format!(
                                "Check command '{}' not found in environment '{}'",
                                base_cmd,
                                self.config.environment()
                            ),
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
            self.validate_single_command(
                &env_config.install,
                &format!("environments.{}.install", env_name),
                result,
            );

            // Validate check command if present
            if let Some(check_cmd) = &env_config.check {
                self.validate_single_command(
                    check_cmd,
                    &format!("environments.{}.check", env_name),
                    result,
                );
            }
        }
    }

    /// Validate a single command syntax
    fn validate_single_command(
        &self,
        command: &str,
        field_name: &str,
        result: &mut ValidationResult,
    ) {
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
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Unmatched single quote in command",
                None,
                Some("Add a closing single quote (') to the command."),
            ));
        }

        if in_double_quotes {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Unmatched double quote in command",
                None,
                Some("Add a closing double quote (\") to the command."),
            ));
        }

        // Check for invalid pipe usage
        if command.contains("| |") {
            result.add_issue(ValidationIssue::error(
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
                result.add_issue(ValidationIssue::warning(
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
            result.add_issue(ValidationIssue::warning(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Contains command substitution with backticks",
                None,
                Some("Consider using $() for command substitution instead of backticks."),
            ));
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
            if let Some(recommendation) =
                self.is_command_recommended_for_env(self.config.environment(), &env_config.install)
            {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::Environment,
                    &format!("environments.{}.install", self.config.environment()),
                    &recommendation,
                    None,
                    Some("Using environment-specific package managers may improve reliability."),
                ));
            }

            // Check for potential issues
            if self.might_require_sudo(&env_config.install) {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", self.config.environment()),
                    "Command might require sudo privileges",
                    None,
                    Some("This command may require administrative privileges to run."),
                ));
            }

            if self.might_download_content(&env_config.install) {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", self.config.environment()),
                    "Command may download content from the internet",
                    None,
                    Some(
                        "This command appears to download content, which may pose security risks.",
                    ),
                ));
            }
        }
    }

    /// Extract the base command from a command string
    pub(crate) fn extract_base_command(command: &str) -> Option<&str> {
        // Simple extraction of the first word before a space, pipe, etc.
        command.split_whitespace().next()
    }

    /// Check if the command might require sudo
    fn might_require_sudo(&self, command: &str) -> bool {
        let sudo_indicators = [
            "sudo ",
            "apt ",
            "apt-get ",
            "dnf ",
            "yum ",
            "pacman ",
            "zypper ",
            "systemctl ",
        ];

        sudo_indicators
            .iter()
            .any(|&indicator| command.contains(indicator))
    }

    /// Check if the command might download content from the internet
    fn might_download_content(&self, command: &str) -> bool {
        let download_indicators = [
            "curl ",
            "wget ",
            "fetch ",
            "git clone",
            "git pull",
            "npm install",
            "pip install",
        ];

        download_indicators
            .iter()
            .any(|&indicator| command.contains(indicator))
    }

    /// Enhanced check for commands specific to particular environments
    fn is_command_recommended_for_env(&self, env_name: &str, command: &str) -> Option<String> {
        // Map of environment prefixes to recommended package managers
        let env_recommendations = [
            // macOS environments
            ("mac", vec!["brew", "port", "mas"]),
            ("darwin", vec!["brew", "port", "mas"]),
            // Linux environments
            ("ubuntu", vec!["apt", "apt-get", "dpkg"]),
            ("debian", vec!["apt", "apt-get", "dpkg"]),
            ("fedora", vec!["dnf", "yum", "rpm"]),
            ("rhel", vec!["dnf", "yum", "rpm"]),
            ("centos", vec!["dnf", "yum", "rpm"]),
            ("arch", vec!["pacman", "yay", "paru"]),
            ("opensuse", vec!["zypper", "rpm"]),
            // Windows environments
            ("windows", vec!["choco", "scoop", "winget"]),
        ];

        // Check if environment matches and command doesn't use recommended package manager
        let env_name_lower = env_name.to_lowercase();

        for (env_pattern, recommended_managers) in &env_recommendations {
            // Check if environment name contains the pattern
            if env_name_lower.contains(env_pattern) {
                // Extract the base command and check if it's in the recommended list
                if let Some(base_cmd) = Self::extract_base_command(command) {
                    if !recommended_managers.iter().any(|&mgr| base_cmd == mgr) {
                        return Some(format!(
                            "Command may not be optimal for '{}' environment. Consider using: {}",
                            env_name,
                            recommended_managers.join(", ")
                        ));
                    }
                }
            }
        }

        None
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

    #[test]
    fn test_validate_valid_package() {
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
        let result = validator.validate_package_by_name("test-package").unwrap();

        assert!(result.is_valid());
        assert_eq!(result.issues.len(), 0);
    }

    #[test]
    fn test_validate_missing_required_fields() {
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

    #[test]
    fn test_validate_invalid_url() {
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
            .unwrap();

        // Should have a URL format error
        let url_errors = result.issues_by_category(&ValidationErrorCategory::UrlFormat);
        assert_eq!(url_errors.len(), 1);
        assert_eq!(url_errors[0].field, "homepage");
    }

    #[test]
    fn test_validate_command_syntax() {
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

    #[test]
    fn test_validate_version_format() {
        let (mut fs, mut runner, config) = setup_test_environment();

        // Add a package with non-semver version
        let yaml = r#"
name: test-package
version: abc
environments:
  test-env:
    install: brew install test-package
"#;
        fs.mock_read_file("/test/packages/bad-version.yaml", yaml);

        runner.mock_is_command_available("brew", true);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator
            .validate_package_file(Path::new("/test/packages/bad-version.yaml"))
            .unwrap();

        // Should have a version format warning (not error)
        let version_issues = result.issues_by_category(&ValidationErrorCategory::InvalidValue);
        assert_eq!(version_issues.len(), 1);
        assert!(version_issues[0].is_warning);
        assert_eq!(version_issues[0].field, "version");
    }

    #[test]
    fn test_validate_missing_environment() {
        let (mut fs, runner, config) = setup_test_environment();

        // Add a package without the current environment
        let yaml = r#"
name: test-package
version: 1.0.0
environments:
  other-env:  # Not the current environment (test-env)
    install: brew install test-package
"#;
        fs.mock_read_file("/test/packages/missing-env.yaml", yaml);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator
            .validate_package_file(Path::new("/test/packages/missing-env.yaml"))
            .unwrap();

        // Should have an environment warning
        let env_issues = result.issues_by_category(&ValidationErrorCategory::Environment);
        assert_eq!(env_issues.len(), 1);
        assert!(env_issues[0].is_warning);
        assert!(env_issues[0].message.contains("'test-env'"));
    }

    #[test]
    fn test_package_not_found() {
        let (mut fs, runner, config) = setup_test_environment();
        fs.mock_path_exists(
            config.expanded_package_directory().join("nonexistent.yaml"),
            false,
        );
        fs.mock_path_exists(
            config.expanded_package_directory().join("nonexistent.yml"),
            false,
        );

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator.validate_package_by_name("nonexistent");

        assert!(matches!(
            result,
            Err(PackageValidatorError::PackageNotFound(_))
        ));
    }

    #[test]
    fn test_multiple_packages_found() {
        let (mut fs, runner, config) = setup_test_environment();

        // Add two files for the same package
        fs.mock_path_exists("/test/packages/duplicate.yaml", true);
        fs.mock_path_exists("/test/packages/duplicate.yml", true);

        let progress_manager = ProgressManager::default();
        let package_repo =
            YamlPackageRepository::new(&fs, config.expanded_package_directory(), &progress_manager);
        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);
        let result = validator.validate_package_by_name("duplicate");

        assert!(matches!(
            result,
            Err(PackageValidatorError::MultiplePackagesFound(_))
        ));
    }

    #[test]
    fn test_validate_package_file() {
        let yaml = r#"
name: test-package
version: 1.0.0
environments:
  other-env:  # Not the current environment (test-env)
    install: brew install test-package
"#;
        let package_path = Path::new("/test/packages/test.yaml");

        let (mut fs, runner, config) = setup_test_environment();
        fs.mock_read_file(&package_path, yaml);

        let package_repo = MockPackageRepository::new();

        let validator = PackageValidator::new(&fs, &runner, &config, &package_repo);

        // This is a simplified test that would need more mocking to validate actual functionality
        let result = validator.validate_package_file(package_path).unwrap();

        // Should fail because the file doesn't exist in our mock filesystem
        pretty_assertions::assert_eq!(
            result,
            ValidationResult {
                package_name: "test-package".to_string(),
                package_path: Some(package_path.into()),
                issues: vec![ValidationIssue {
                    category: ValidationErrorCategory::Environment,
                    field: "environments".to_string(),
                    message: "Current environment 'test-env' is not configured".to_string(),
                    line: None,
                    is_warning: true,
                    suggestion: Some(
                        "Add an environment section for 'test-env' if needed for this environment."
                            .to_string()
                    )
                }],
                package: Some(Package::from_yaml(yaml).unwrap())
            }
        );
    }
}
