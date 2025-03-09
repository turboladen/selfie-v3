// src/services/validation_command.rs
use crate::{
    adapters::{
        package_repo::yaml::YamlPackageRepository,
        progress::{ProgressManager, ProgressStyleType},
    },
    domain::{application::commands::PackageCommand, config::AppConfig},
    ports::{command::CommandRunner, filesystem::FileSystem},
    services::package_validator::PackageValidator,
};

/// Result of running the validate command
#[derive(Debug)]
pub enum ValidationCommandResult {
    /// Package validation successful (may include warnings)
    Valid(String),
    /// Package validation failed with errors
    Invalid(String),
    /// Command failed to run
    Error(String),
}

/// Handles the 'package validate' command
pub struct ValidationCommand<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    runner: &'a R,
    config: &'a AppConfig,
    progress_manager: &'a ProgressManager,
}

impl<'a, F: FileSystem, R: CommandRunner> ValidationCommand<'a, F, R> {
    /// Create a new validate command handler
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: &'a AppConfig,
        progress_manager: &'a ProgressManager,
    ) -> Self {
        Self {
            fs,
            runner,
            config,
            progress_manager,
        }
    }

    /// Execute the validate command
    pub fn execute(&self, cmd: &PackageCommand) -> ValidationCommandResult {
        match cmd {
            PackageCommand::Validate {
                package_name,
                package_path,
            } => {
                // Create progress display
                let progress = self.progress_manager.create_progress_bar(
                    &format!("Validating package '{}'", package_name),
                    ProgressStyleType::Spinner,
                );

                // Create package repository
                let package_repo =
                    YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());

                // Create the enhanced validator
                let validator =
                    PackageValidator::new(self.fs, self.runner, self.config, &package_repo);

                // Validate package
                let result = if let Some(path) = package_path {
                    validator.validate_package_file(path)
                } else {
                    validator.validate_package_by_name(package_name)
                };

                match result {
                    Ok(validation_result) => {
                        // Format the validation result
                        let formatted =
                            validation_result.format_validation_result(self.progress_manager);

                        if validation_result.is_valid() {
                            progress.finish_with_message("Validation successful");
                            ValidationCommandResult::Valid(formatted)
                        } else {
                            progress.abandon_with_message("Validation failed");
                            ValidationCommandResult::Invalid(formatted)
                        }
                    }
                    Err(err) => {
                        // More verbose error handling
                        if self.config.verbose() {
                            progress.println(format!("Error details: {:#?}", err));
                        }

                        progress.abandon_with_message("Validation failed");

                        let e = err;
                        ValidationCommandResult::Error(format!("Error: {}", e))
                    }
                }
            }
            _ => ValidationCommandResult::Error(
                "Invalid command. Expected 'validate <package-name>'".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::Path;

    use super::*;
    use crate::{
        domain::{application::commands::PackageCommand, config::AppConfigBuilder},
        ports::{command::MockCommandRunner, filesystem::MockFileSystem},
    };

    #[test]
    fn test_execute_validation_command() {
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

        let cmd = ValidationCommand::new(&fs, &runner, &config, &progress_manager);

        let package_cmd = PackageCommand::Validate {
            package_name: "test-package".to_string(),
            package_path: None,
        };

        // This would need to be more thoroughly mocked to test actual validation
        // For now we're just testing that the command structure works
        let result = cmd.execute(&package_cmd);

        match result {
            ValidationCommandResult::Invalid(_) => {
                // Expected for this simple test
            }
            _ => panic!("Expected error result due to lack of mocking"),
        }
    }
}
