// src/services/validation_service.rs
use std::{os::unix::fs::PermissionsExt, path::Path};

use console::style;

use crate::{
    domain::{
        config::Config,
        package::Package,
        validation::{ValidationError, ValidationErrorCategory, ValidationIssue, ValidationResult},
    },
    ports::{
        command::CommandRunner,
        filesystem::FileSystem,
        package_repo::{PackageRepoError, PackageRepository},
    },
};

/// Service that handles package validation with external dependencies
pub struct ValidationService<'a, F: FileSystem, R: CommandRunner, P: PackageRepository> {
    fs: &'a F,
    runner: &'a R,
    config: &'a Config,
    package_repo: &'a P,
}

impl<'a, F: FileSystem, R: CommandRunner, P: PackageRepository> ValidationService<'a, F, R, P> {
    /// Create a new ValidationService
    pub fn new(fs: &'a F, runner: &'a R, config: &'a Config, package_repo: &'a P) -> Self {
        Self {
            fs,
            runner,
            config,
            package_repo,
        }
    }

    /// Validate a package by name
    pub fn validate_package_by_name(
        &self,
        name: &str,
    ) -> Result<ValidationResult, ValidationError> {
        // Find the package files
        let package_files = self
            .package_repo
            .find_package_files(name)
            .map_err(|e| match e {
                PackageRepoError::PackageNotFound(name) => ValidationError::PackageNotFound(name),
                PackageRepoError::MultiplePackagesFound(name) => {
                    ValidationError::MultiplePackagesFound(name)
                }
                _ => ValidationError::FileSystemError(e.to_string()),
            })?;

        if package_files.is_empty() {
            return Err(ValidationError::PackageNotFound(name.to_string()));
        }

        if package_files.len() > 1 {
            return Err(ValidationError::MultiplePackagesFound(name.to_string()));
        }

        // Validate the first package file
        self.validate_package_file(&package_files[0])
    }

    /// Validate a specific package file
    pub fn validate_package_file(&self, path: &Path) -> Result<ValidationResult, ValidationError> {
        // Read and parse the package file
        let file_content = self
            .fs
            .read_file(path)
            .map_err(|e| ValidationError::FileSystemError(e.to_string()))?;

        // Try to parse the package, but continue even if it fails
        let package = Package::from_yaml(&file_content);

        // Get the package name either from the parsed package or the file name
        let package_name = match &package {
            Ok(pkg) => pkg.name.clone(),
            Err(_) => path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
        };

        let mut result = ValidationResult::new(&package_name).with_path(path.to_path_buf());

        // If parsing failed, add the parse error and return early
        match package {
            Ok(pkg) => {
                // Start with domain validation
                let domain_issues = pkg.validate(&self.config.environment);
                result.add_issues(domain_issues);

                // Add enhanced validation
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

    /// Enhance validation with checks that require external dependencies
    fn enhance_validation(&self, package: &Package, result: &mut ValidationResult) {
        // Add command availability checks
        self.validate_command_availability(package, result);

        // Add environment-specific recommendations
        self.validate_environment_recommendations(package, result);
    }

    /// Validate command availability using the command runner
    fn validate_command_availability(&self, package: &Package, result: &mut ValidationResult) {
        // We only check commands for the current environment
        if let Some(env_config) = package.environments.get(&self.config.environment) {
            // Extract base command from install command
            if let Some(base_cmd) = Self::extract_base_command(&env_config.install) {
                let is_available = self.runner.is_command_available(base_cmd);

                if !is_available {
                    result.add_issue(ValidationIssue::warning(
                        ValidationErrorCategory::Availability,
                        &format!("environments.{}.install", self.config.environment),
                        &format!(
                            "Command '{}' not found in environment '{}'",
                            base_cmd, self.config.environment
                        ),
                        None,
                        Some("Install the command before using this package."),
                    ));
                }
            }

            // Check check command if present
            if let Some(check_cmd) = &env_config.check {
                if let Some(base_cmd) = Self::extract_base_command(check_cmd) {
                    let is_available = self.runner.is_command_available(base_cmd);

                    if !is_available {
                        result.add_issue(ValidationIssue::warning(
                            ValidationErrorCategory::Availability,
                            &format!("environments.{}.check", self.config.environment),
                            &format!(
                                "Check command '{}' not found in environment '{}'",
                                base_cmd, self.config.environment
                            ),
                            None,
                            Some("Install the command before using this package."),
                        ));
                    }
                }
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
        if let Some(env_config) = package.environments.get(&self.config.environment) {
            if let Some(recommendation) =
                self.is_command_recommended_for_env(&self.config.environment, &env_config.install)
            {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::Environment,
                    &format!("environments.{}.install", self.config.environment),
                    &recommendation,
                    None,
                    Some("Using environment-specific package managers may improve reliability."),
                ));
            }

            // Check for potential issues
            if self.might_require_sudo(&env_config.install) {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", self.config.environment),
                    "Command might require sudo privileges",
                    None,
                    Some("This command may require administrative privileges to run."),
                ));
            }

            if self.might_download_content(&env_config.install) {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    &format!("environments.{}.install", self.config.environment),
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
    fn extract_base_command(command: &str) -> Option<&str> {
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

    /// Format validation result for display, with optional color
    pub fn format_validation_result(
        &self,
        result: &ValidationResult,
        use_colors: bool,
        verbose: bool,
    ) -> String {
        let mut output = String::new();

        if result.is_valid() {
            let status = if use_colors {
                style("✓").green().to_string()
            } else {
                "✓".to_string()
            };

            let package_name = if use_colors {
                style(&result.package_name).magenta().bold().to_string()
            } else {
                result.package_name.clone()
            };

            output.push_str(&format!("{} Package '{}' is valid\n", status, package_name));

            // Add warnings if any
            let warnings = result.warnings();
            if !warnings.is_empty() {
                let warning_header = if use_colors {
                    style("Warnings:").yellow().bold().to_string()
                } else {
                    "Warnings:".to_string()
                };

                output.push_str(&format!("\n{}\n", warning_header));

                for warning in warnings {
                    let warn_prefix = if use_colors {
                        style("  ! ").yellow().to_string()
                    } else {
                        "  ! ".to_string()
                    };

                    output.push_str(&format!(
                        "{}{}: {}\n",
                        warn_prefix, warning.field, warning.message
                    ));

                    if let Some(suggestion) = &warning.suggestion {
                        let suggestion_text = if use_colors {
                            style(format!("    Suggestion: {}", suggestion))
                                .dim()
                                .to_string()
                        } else {
                            format!("    Suggestion: {}", suggestion)
                        };
                        output.push_str(&format!("{}\n", suggestion_text));
                    }
                }
            }
        } else {
            let status = if use_colors {
                style("✗").red().bold().to_string()
            } else {
                "✗".to_string()
            };

            let package_name = if use_colors {
                style(&result.package_name).magenta().bold().to_string()
            } else {
                result.package_name.clone()
            };

            output.push_str(&format!(
                "{} Validation failed for package: {}\n",
                status, package_name
            ));

            // Group errors by category for better organization
            {
                let errors = result.errors();
                for category in &[
                    ValidationErrorCategory::RequiredField,
                    ValidationErrorCategory::InvalidValue,
                    ValidationErrorCategory::Environment,
                    ValidationErrorCategory::CommandSyntax,
                    ValidationErrorCategory::UrlFormat,
                    ValidationErrorCategory::FileSystem,
                    ValidationErrorCategory::Availability,
                    ValidationErrorCategory::Other,
                ] {
                    let category_errors: Vec<_> = errors
                        .iter()
                        .filter(|e| e.category == *category)
                        .cloned()
                        .collect();

                    if !category_errors.is_empty() {
                        let header = if use_colors {
                            style(format!("\n{} errors:", category))
                                .red()
                                .bold()
                                .to_string()
                        } else {
                            format!("\n{} errors:", category)
                        };

                        output.push_str(&header);
                        output.push('\n');

                        for error in &category_errors {
                            let field = if use_colors {
                                style(&error.field).cyan().to_string()
                            } else {
                                error.field.clone()
                            };

                            output.push_str(&format!("  • {}: {}\n", field, error.message));

                            if let Some(suggestion) = &error.suggestion {
                                let suggestion_text = if use_colors {
                                    style(format!("    Suggestion: {}", suggestion))
                                        .dim()
                                        .to_string()
                                } else {
                                    format!("    Suggestion: {}", suggestion)
                                };
                                output.push_str(&format!("{}\n", suggestion_text));
                            }
                        }
                    }
                }
            }

            // Warnings
            {
                let warnings = result.warnings();
                for category in &[
                    ValidationErrorCategory::RequiredField,
                    ValidationErrorCategory::InvalidValue,
                    ValidationErrorCategory::Environment,
                    ValidationErrorCategory::CommandSyntax,
                    ValidationErrorCategory::UrlFormat,
                    ValidationErrorCategory::FileSystem,
                    ValidationErrorCategory::Availability,
                    ValidationErrorCategory::Other,
                ] {
                    let category_errors: Vec<_> = warnings
                        .iter()
                        .filter(|e| e.category == *category)
                        .cloned()
                        .collect();

                    if !category_errors.is_empty() {
                        let header = if use_colors {
                            style(format!("\n{:?} warnings:", category))
                                .red()
                                .bold()
                                .to_string()
                        } else {
                            format!("\n{:?} warnings:", category)
                        };

                        output.push_str(&header);
                        output.push('\n');

                        for error in &category_errors {
                            let field = if use_colors {
                                style(&error.field).cyan().to_string()
                            } else {
                                error.field.clone()
                            };

                            output.push_str(&format!("  • {}: {}\n", field, error.message));

                            if let Some(suggestion) = &error.suggestion {
                                let suggestion_text = if use_colors {
                                    style(format!("    Suggestion: {}", suggestion))
                                        .dim()
                                        .to_string()
                                } else {
                                    format!("    Suggestion: {}", suggestion)
                                };
                                output.push_str(&format!("{}\n", suggestion_text));
                            }
                        }
                    }
                }
            }
        }

        // Show file path
        if let Some(path) = &result.package_path {
            let path_text = if use_colors {
                style(format!(
                    "\nYou can find the package file at: {}",
                    path.display()
                ))
                .dim()
                .to_string()
            } else {
                format!("\nYou can find the package file at: {}", path.display())
            };

            output.push_str(&format!("{}\n", path_text));
        }

        // Add verbose information if requested
        if verbose {
            self.add_verbose_information(result, &mut output, use_colors);
        }

        output
    }

    /// Add verbose information to the output
    fn add_verbose_information(
        &self,
        result: &ValidationResult,
        output: &mut String,
        use_colors: bool,
    ) {
        output.push_str("\n--- Verbose Information ---\n");

        // Add file details
        if let Some(path) = &result.package_path {
            if self.fs.path_exists(path) {
                output.push_str("\nPackage file details:\n");
                output.push_str(&format!("  Path: {}\n", path.display()));

                // Try to get file metadata
                match path.metadata() {
                    Ok(metadata) => {
                        output.push_str(&format!(
                            "  Permissions: {:o}\n",
                            metadata.permissions().mode()
                        ));

                        if let Ok(modified) = metadata.modified() {
                            output.push_str(&format!("  Last modified: {:?}\n", modified));
                        }

                        if let Ok(created) = metadata.created() {
                            output.push_str(&format!("  Created: {:?}\n", created));
                        }

                        output.push_str(&format!("  Size: {} bytes\n", metadata.len()));
                    }
                    Err(_) => {
                        output.push_str("  (Unable to read file metadata)\n");
                    }
                }
            }
        }

        // Add package details
        if let Some(package) = &result.package {
            output.push_str("\nPackage structure details:\n");
            output.push_str(&format!("  Name: {}\n", package.name));
            output.push_str(&format!("  Version: {}\n", package.version));

            if let Some(homepage) = &package.homepage {
                output.push_str(&format!("  Homepage: {}\n", homepage));
            }

            if let Some(description) = &package.description {
                output.push_str(&format!("  Description: {}\n", description));
            }

            output.push_str("\n  Environments:\n");
            for (name, env) in &package.environments {
                let is_current = name == &self.config.environment;
                let env_header = if is_current {
                    if use_colors {
                        format!("    {} (current):", style(name).green().bold())
                    } else {
                        format!("    {} (current):", name)
                    }
                } else {
                    format!("    {}:", name)
                };

                output.push_str(&format!("{}\n", env_header));
                output.push_str(&format!("      Install: {}\n", env.install));

                if let Some(check) = &env.check {
                    output.push_str(&format!("      Check: {}\n", check));
                }

                if !env.dependencies.is_empty() {
                    output.push_str("      Dependencies:\n");
                    for dep in &env.dependencies {
                        output.push_str(&format!("        - {}\n", dep));
                    }
                }
            }
        }

        // Add validation statistics
        output.push_str("\nValidation statistics:\n");
        output.push_str(&format!("  Total issues: {}\n", result.issues.len()));
        output.push_str(&format!("  Errors: {}\n", result.errors().len()));
        output.push_str(&format!("  Warnings: {}\n", result.warnings().len()));

        // Issues by category
        output.push_str("  Issues by category:\n");
        for category in &[
            ValidationErrorCategory::RequiredField,
            ValidationErrorCategory::CommandSyntax,
            ValidationErrorCategory::UrlFormat,
            ValidationErrorCategory::Environment,
            ValidationErrorCategory::Availability,
            ValidationErrorCategory::Other,
        ] {
            let count = result.issues_by_category(category).len();
            if count > 0 {
                output.push_str(&format!("    {:?}: {}\n", category, count));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::config::ConfigBuilder,
        ports::{
            command::MockCommandRunner, filesystem::MockFileSystem,
            package_repo::MockPackageRepository,
        },
    };
    use std::path::{Path, PathBuf};

    fn setup_test_environment() -> (
        MockFileSystem,
        MockCommandRunner,
        Config,
        MockPackageRepository,
    ) {
        let fs = MockFileSystem::default();
        let runner = MockCommandRunner::new();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();
        let package_repo = MockPackageRepository::new();
        (fs, runner, config, package_repo)
    }

    #[test]
    fn test_validate_package_by_name_not_found() {
        let (fs, runner, config, mut package_repo) = setup_test_environment();

        // Mock package repo to return not found
        package_repo
            .expect_find_package_files()
            .returning(|_| Ok(Vec::new()));

        let service = ValidationService::new(&fs, &runner, &config, &package_repo);
        let result = service.validate_package_by_name("nonexistent");

        assert!(matches!(result, Err(ValidationError::PackageNotFound(_))));
    }

    #[test]
    fn test_validate_package_by_name_multiple_found() {
        let (fs, runner, config, mut package_repo) = setup_test_environment();

        // Mock package repo to return multiple files
        package_repo.expect_find_package_files().returning(|_| {
            Ok(vec![
                PathBuf::from("/test/packages/test.yaml"),
                PathBuf::from("/test/packages/test.yml"),
            ])
        });

        let service = ValidationService::new(&fs, &runner, &config, &package_repo);
        let result = service.validate_package_by_name("test");

        assert!(matches!(
            result,
            Err(ValidationError::MultiplePackagesFound(_))
        ));
    }

    #[test]
    fn test_validate_package_file_parse_error() {
        let (mut fs, runner, config, package_repo) = setup_test_environment();

        // Mock file system to return invalid YAML
        let path = Path::new("/test/packages/invalid.yaml");
        fs.mock_read_file(path, "invalid: yaml: :");

        let service = ValidationService::new(&fs, &runner, &config, &package_repo);
        let result = service.validate_package_file(path);

        assert!(result.is_ok());
        let validation_result = result.unwrap();
        assert!(!validation_result.is_valid());
        assert!(validation_result
            .issues
            .iter()
            .any(|i| i.category == ValidationErrorCategory::Other
                && i.message.contains("Failed to parse")));
    }

    #[test]
    fn test_validate_command_availability() {
        let (mut fs, mut runner, config, package_repo) = setup_test_environment();

        // Create a valid package with commands
        let yaml = r#"
            name: test-package
            version: 1.0.0
            environments:
              test-env:
                install: available-cmd install
                check: missing-cmd check
        "#;

        let path = Path::new("/test/packages/test.yaml");
        fs.mock_read_file(path, yaml);

        // Mock command runner
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("available-cmd"))
            .returning(|_| true);

        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("missing-cmd"))
            .returning(|_| false);

        let service = ValidationService::new(&fs, &runner, &config, &package_repo);
        let result = service.validate_package_file(path);

        assert!(result.is_ok());
        let validation_result = result.unwrap();

        // Should have a warning for missing-cmd
        assert!(validation_result
            .issues
            .iter()
            .any(|i| i.category == ValidationErrorCategory::Availability
                && i.message.contains("missing-cmd")
                && i.is_warning));

        // Should not have a warning for available-cmd
        assert!(!validation_result
            .issues
            .iter()
            .any(|i| i.category == ValidationErrorCategory::Availability
                && i.message.contains("available-cmd")));
    }

    #[test]
    fn test_environment_recommendations() {
        let (mut fs, mut runner, _config, package_repo) = setup_test_environment();

        // Create a package with non-optimal commands for the environment
        let yaml = r#"
            name: test-package
            version: 1.0.0
            environments:
              test-env:
                install: apt install package
        "#;

        let path = Path::new("/test/packages/test.yaml");
        fs.mock_read_file(path, yaml);

        // Override config to use mac environment
        let mac_config = ConfigBuilder::default()
            .environment("mac-env")
            .package_directory("/test/packages")
            .build();

        let yaml_mac = r#"
            name: test-package
            version: 1.0.0
            environments:
              mac-env:
                install: apt install package
        "#;

        let mac_path = Path::new("/test/packages/mac.yaml");
        fs.mock_read_file(mac_path, yaml_mac);

        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("apt"))
            .returning(|_| true);

        let service = ValidationService::new(&fs, &runner, &mac_config, &package_repo);
        let result = service.validate_package_file(mac_path);

        assert!(result.is_ok());
        let validation_result = result.unwrap();

        // Should recommend brew for mac environment
        assert!(validation_result.issues.iter().any(|i| i.category
            == ValidationErrorCategory::Environment
            && i.message.contains("not be optimal")
            && i.message.contains("brew")
            && i.is_warning));
    }

    #[test]
    fn test_might_require_sudo() {
        let (fs, runner, config, package_repo) = setup_test_environment();
        let service = ValidationService::new(&fs, &runner, &config, &package_repo);

        assert!(service.might_require_sudo("sudo apt install package"));
        assert!(service.might_require_sudo("apt install package"));
        assert!(service.might_require_sudo("pacman -S package"));
        assert!(!service.might_require_sudo("brew install package"));
        assert!(!service.might_require_sudo("cargo install package"));
    }

    #[test]
    fn test_might_download_content() {
        let (fs, runner, config, package_repo) = setup_test_environment();
        let service = ValidationService::new(&fs, &runner, &config, &package_repo);

        assert!(service.might_download_content("curl -O https://example.com/file"));
        assert!(service.might_download_content("wget https://example.com/file"));
        assert!(service.might_download_content("git clone https://github.com/user/repo"));
        assert!(service.might_download_content("npm install express"));
        assert!(!service.might_download_content("ls -la"));
        assert!(!service.might_download_content("echo test"));
    }

    #[test]
    fn test_format_validation_result() {
        let (fs, runner, config, package_repo) = setup_test_environment();
        let service = ValidationService::new(&fs, &runner, &config, &package_repo);

        // Create a test validation result
        let mut result = ValidationResult::new("test-package");
        result = result.with_path(PathBuf::from("/test/packages/test.yaml"));

        // Add some issues
        result.add_issue(ValidationIssue::error(
            ValidationErrorCategory::RequiredField,
            "name",
            "Package name is required",
            None,
            Some("Add a name field"),
        ));

        result.add_issue(ValidationIssue::warning(
            ValidationErrorCategory::CommandSyntax,
            "install",
            "Command uses backticks",
            None,
            Some("Use $() instead"),
        ));

        // Format the result
        let formatted = service.format_validation_result(&result, false, false);

        dbg!(&formatted);
        // Verify the output
        assert!(formatted.contains("Validation failed"));
        assert!(formatted.contains("Package name is required"));
        assert!(formatted.contains("Command uses backticks"));
        assert!(formatted.contains("Add a name field"));
        assert!(formatted.contains("Use $() instead"));
        assert!(formatted.contains("/test/packages/test.yaml"));
    }
}
