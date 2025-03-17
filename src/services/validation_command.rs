use std::path::Path;

// src/services/validation_command.rs
use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
    domain::config::AppConfig,
    ports::filesystem::FileSystem,
    services::package::validate::PackageValidator,
};

use super::command_validator::CommandValidator;

/// Result of running the validate command
#[derive(Debug)]
pub(crate) enum ValidationCommandResult {
    /// Package validation successful (may include warnings)
    Valid(String),
    /// Package validation failed with errors
    Invalid(String),
    /// Command failed to run
    Error(String),
}

/// Handles the 'package validate' command
pub(crate) struct ValidationCommand<'a> {
    fs: &'a dyn FileSystem,
    config: &'a AppConfig,
    progress_manager: &'a ProgressManager,
    command_validator: &'a CommandValidator<'a>,
}

impl<'a> ValidationCommand<'a> {
    /// Create a new validate command handler
    pub(crate) fn new(
        fs: &'a dyn FileSystem,
        config: &'a AppConfig,
        progress_manager: &'a ProgressManager,
        command_validator: &'a CommandValidator,
    ) -> Self {
        Self {
            fs,
            config,
            progress_manager,
            command_validator,
        }
    }

    /// Execute the validate command
    pub(crate) async fn execute(
        &self,
        package_name: &str,
        package_path: Option<&Path>,
    ) -> ValidationCommandResult {
        self.progress_manager
            .print_progress(format!("Validating package '{}'", package_name));

        // Create package repository
        let package_repo = YamlPackageRepository::new(
            self.fs,
            self.config.expanded_package_directory(),
            self.progress_manager,
        );

        // Create the enhanced validator
        let validator =
            PackageValidator::new(self.fs, self.config, &package_repo, self.command_validator);

        // Validate package
        let result = if let Some(path) = package_path {
            validator.validate_package_file(path).await
        } else {
            validator.validate_package_by_name(package_name).await
        };

        match result {
            Ok(validation_result) => {
                // Format the validation result
                let formatted = validation_result.format_validation_result(self.progress_manager);

                if validation_result.is_valid() {
                    self.progress_manager.print_success("Validation successful");
                    ValidationCommandResult::Valid(formatted)
                } else {
                    self.progress_manager.print_error("Validation failed");
                    ValidationCommandResult::Invalid(formatted)
                }
            }
            Err(err) => {
                // More verbose error handling
                if self.config.verbose() {
                    self.progress_manager
                        .print_progress(format!("Error details: {:#?}", err));
                }

                self.progress_manager.print_error("Validation failed");

                let e = err;
                ValidationCommandResult::Error(format!("Error: {}", e))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{
        domain::config::AppConfigBuilder,
        ports::{command::MockCommandRunner, filesystem::MockFileSystem},
    };

    #[tokio::test]
    async fn test_execute_validation_command() {
        let package_dir = Path::new("/test/packages");

        let mut fs = MockFileSystem::default();
        fs.mock_path_exists(package_dir, true);
        fs.mock_path_exists(package_dir.join("test-package.yaml"), true);
        fs.mock_path_exists(package_dir.join("test-package.yml"), false);
        fs.mock_read_file(
            package_dir.join("test-package.yaml"),
            r#"---
        name: test-package
        # version:
        homepage:
        description: 
        environmentttttttt:
          cool-computer:
            install:
            check: sad face
            "#,
        );

        let runner = MockCommandRunner::new();

        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory(package_dir)
            .verbose(true)
            .build();

        let progress_manager = ProgressManager::from(&config);
        let command_validator = CommandValidator::new(&runner);

        let cmd = ValidationCommand::new(&fs, &config, &progress_manager, &command_validator);

        // This would need to be more thoroughly mocked to test actual validation
        // For now we're just testing that the command structure works
        let result = cmd.execute("test-package", None).await;

        match result {
            ValidationCommandResult::Invalid(_) => {
                // Expected for this simple test
            }
            _ => panic!("Expected error result due to lack of mocking"),
        }
    }

    #[tokio::test]
    async fn test_validation_integration() {
        // Set up test environment
        let mut fs = MockFileSystem::default();
        let mut runner = MockCommandRunner::new();

        // Create config
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        // Set up package directory
        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(&package_dir, true);

        // Create a valid package file
        let valid_yaml = r#"
        name: valid-package
        version: 1.0.0
        homepage: https://example.com
        description: A valid test package
        environments:
          test-env:
            install: echo test
            check: which test
    "#;

        let valid_path = package_dir.join("valid-package.yaml");
        fs.mock_path_exists(&valid_path, true);
        fs.mock_path_exists(package_dir.join("valid-package.yml"), false);
        fs.mock_read_file(&valid_path, valid_yaml);

        // Create an invalid package file
        let invalid_yaml = r#"
        name: ""
        version: ""
        homepage: invalid-url
        environments:
          other-env:
            install: ""
    "#;

        let invalid_path = package_dir.join("invalid-package.yaml");
        fs.mock_path_exists(&invalid_path, true);
        fs.mock_path_exists(package_dir.join("invalid-package.yml"), false);
        fs.mock_read_file(&invalid_path, invalid_yaml);

        // Set up command runner
        runner.mock_is_command_available("echo", true);

        runner.mock_is_command_available("which", true);

        let progress_manager = ProgressManager::default();
        let command_validator = CommandValidator::new(&runner);
        let command = ValidationCommand::new(&fs, &config, &progress_manager, &command_validator);

        // Test validation on valid package
        let result = command.execute("valid-package", None).await;
        match result {
            ValidationCommandResult::Valid(output) => {
                assert!(output.contains("valid-package"));
                assert!(output.contains("is valid"));
            }
            _ => panic!("Expected Valid result"),
        }

        // Test validation on invalid package
        let result = command.execute("invalid-package", None).await;
        match result {
            ValidationCommandResult::Invalid(output) => {
                assert!(output.contains("invalid-package"));
                assert!(output.contains("Validation failed"));
                assert!(output.contains("Required field errors"));
                assert!(output.contains("URL format errors"));
            }
            _ => panic!("Expected Invalid result"),
        }

        // Test validation with path parameter
        let result = command.execute("any-name", Some(&valid_path)).await;
        match result {
            ValidationCommandResult::Valid(_) => {
                // Expected result
            }
            _ => panic!("Expected Valid result for path validation"),
        }
    }
}
