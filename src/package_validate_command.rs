// src/package_validate_command.rs
// Implements the 'selfie package validate' command

use std::path::Path;

use crate::{
    cli::PackageSubcommands,
    command::CommandRunner,
    config::Config,
    filesystem::FileSystem,
    package_validator::{format_validation_result, PackageValidator, PackageValidatorError},
    progress_display::{ProgressManager, ProgressStyleType},
};

/// Result of running the validate command
pub enum ValidateCommandResult {
    /// Package validation successful (may include warnings)
    Valid(String),
    /// Package validation failed with errors
    Invalid(String),
    /// Command failed to run
    Error(String),
}

/// Handles the 'package validate' command
pub struct ValidateCommand<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    _runner: &'a R,
    config: Config,
    progress_manager: &'a ProgressManager,
    verbose: bool,
}

impl<'a, F: FileSystem, R: CommandRunner> ValidateCommand<'a, F, R> {
    /// Create a new validate command handler
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: Config,
        progress_manager: &'a ProgressManager,
        verbose: bool,
    ) -> Self {
        Self {
            fs,
            _runner: runner,
            config,
            progress_manager,
            verbose,
        }
    }

    /// Execute the validate command
    pub fn execute(&self, cmd: &PackageSubcommands) -> ValidateCommandResult {
        match cmd {
            PackageSubcommands::Validate {
                package_name,
                package_path,
            } => {
                // Create progress display
                let progress = self.progress_manager.create_progress_bar(
                    "validate",
                    &format!("Validating package '{}'", package_name),
                    ProgressStyleType::Spinner,
                );

                // Create validator
                let validator = PackageValidator::new(self.fs, &self.config);

                // Validate package
                let result = if let Some(path) = package_path {
                    validator.validate_package_file(path)
                } else {
                    validator.validate_package(package_name)
                };

                match result {
                    Ok(validation_result) => {
                        // Format validation result
                        let formatted = format_validation_result(&validation_result);

                        if validation_result.is_valid() {
                            progress.finish_with_message("Validation successful");
                            ValidateCommandResult::Valid(formatted)
                        } else {
                            progress.abandon_with_message("Validation failed");
                            ValidateCommandResult::Invalid(formatted)
                        }
                    }
                    Err(err) => {
                        progress.abandon_with_message(format!("Validation error: {}", err));
                        match err {
                            PackageValidatorError::PackageNotFound(name) => {
                                ValidateCommandResult::Error(format!(
                                    "Package '{}' not found\n\nVerify the package name and make sure the package file exists in the package directory.",
                                    name
                                ))
                            }
                            PackageValidatorError::MultiplePackagesFound(name) => {
                                ValidateCommandResult::Error(format!(
                                    "Multiple package files found for '{}'\n\nUse the --package-path flag to specify which file to validate.",
                                    name
                                ))
                            }
                            _ => ValidateCommandResult::Error(format!("Error: {}", err)),
                        }
                    }
                }
            }
            _ => ValidateCommandResult::Error(
                "Invalid command. Expected 'validate <package-name>'".to_string(),
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command::mock::MockCommandRunner,
        config::ConfigBuilder,
        filesystem::mock::MockFileSystem,
        progress::ConsoleRenderer,
    };
    use std::path::PathBuf;

    fn setup_test_environment() -> (
        MockFileSystem,
        MockCommandRunner,
        Config,
        ProgressManager,
    ) {
        let fs = MockFileSystem::default();
        let runner = MockCommandRunner::new();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();
        let progress_manager = ProgressManager::new(false, false, true);

        // Add the package directory to the filesystem
        fs.add_existing_path(Path::new("/test/packages"));

        (fs, runner, config, progress_manager)
    }

    fn create_test_yaml(valid: bool) -> String {
        if valid {
            r#"
name: test-package
version: 1.0.0
environments:
  test-env:
    install: brew install test-package
"#
            .to_string()
        } else {
            r#"
# Missing name
version: 1.0.0
environments:
  test-env:
    install: brew install test-package
"#
            .to_string()
        }
    }

    #[test]
    fn test_validate_valid_package() {
        let (fs, runner, config, progress_manager) = setup_test_environment();

        // Add a valid package file
        let yaml = create_test_yaml(true);
        fs.add_file(Path::new("/test/packages/test-package.yaml"), &yaml);

        let cmd = PackageSubcommands::Validate {
            package_name: "test-package".to_string(),
            package_path: None,
        };

        let validate_cmd = ValidateCommand::new(&fs, &runner, config, &progress_manager, false);
        let result = validate_cmd.execute(&cmd);

        match result {
            ValidateCommandResult::Valid(output) => {
                assert!(output.contains("valid"));
            }
            _ => panic!("Expected Valid result"),
        }
    }

    #[test]
    fn test_validate_invalid_package() {
        let (fs, runner, config, progress_manager) = setup_test_environment();

        // Add an invalid package file
        let yaml = create_test_yaml(false);
        fs.add_file(Path::new("/test/packages/invalid.yaml"), &yaml);

        let cmd = PackageSubcommands::Validate {
            package_name: "invalid".to_string(),
            package_path: None,
        };

        let validate_cmd = ValidateCommand::new(&fs, &runner, config, &progress_manager, false);
        let result = validate_cmd.execute(&cmd);

        match result {
            ValidateCommandResult::Invalid(output) => {
                assert!(output.contains("failed"));
                assert!(output.contains("name"));
            }
            _ => panic!("Expected Invalid result"),
        }
    }

    #[test]
    fn test_validate_nonexistent_package() {
        let (fs, runner, config, progress_manager) = setup_test_environment();

        let cmd = PackageSubcommands::Validate {
            package_name: "nonexistent".to_string(),
            package_path: None,
        };

        let validate_cmd = ValidateCommand::new(&fs, &runner, config, &progress_manager, false);
        let result = validate_cmd.execute(&cmd);

        match result {
            ValidateCommandResult::Error(output) => {
                assert!(output.contains("not found"));
            }
            _ => panic!("Expected Error result"),
        }
    }

    #[test]
    fn test_validate_with_specific_path() {
        let (fs, runner, config, progress_manager) = setup_test_environment();

        // Add a package file in a non-standard location
        let yaml = create_test_yaml(true);
        fs.add_file(Path::new("/other/location/test-package.yaml"), &yaml);
        fs.add_existing_path(Path::new("/other/location"));

        let cmd = PackageSubcommands::Validate {
            package_name: "test-package".to_string(),
            package_path: Some(PathBuf::from("/other/location/test-package.yaml")),
        };

        let validate_cmd = ValidateCommand::new(&fs, &runner, config, &progress_manager, false);
        let result = validate_cmd.execute(&cmd);

        match result {
            ValidateCommandResult::Valid(output) => {
                assert!(output.contains("valid"));
            }
            _ => panic!("Expected Valid result"),
        }
    }

    #[test]
    fn test_validate_multiple_packages() {
        let (fs, runner, config, progress_manager) = setup_test_environment();

        // Add two files for the same package
        let yaml = create_test_yaml(true);
        fs.add_file(Path::new("/test/packages/duplicate.yaml"), &yaml);
        fs.add_file(Path::new("/test/packages/duplicate.yml"), &yaml);

        let cmd = PackageSubcommands::Validate {
            package_name: "duplicate".to_string(),
            package_path: None,
        };

        let validate_cmd = ValidateCommand::new(&fs, &runner, config, &progress_manager, false);
        let result = validate_cmd.execute(&cmd);

        match result {
            ValidateCommandResult::Error(output) => {
                assert!(output.contains("Multiple package files found"));
                assert!(output.contains("--package-path"));
            }
            _ => panic!("Expected Error result"),
        }
    }
}
