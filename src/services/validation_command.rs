// src/services/validation_command.rs
use crate::{
    adapters::{
        package_repo::yaml::YamlPackageRepository,
        progress::{ProgressManager, ProgressStyleType},
    },
    domain::{application::commands::PackageCommand, config::Config},
    ports::{command::CommandRunner, filesystem::FileSystem},
    services::package_validator::PackageValidator,
};

/// Result of running the validate command
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
    config: Config,
    progress_manager: &'a ProgressManager,
    verbose: bool,
}

impl<'a, F: FileSystem, R: CommandRunner> ValidationCommand<'a, F, R> {
    /// Create a new validate command handler
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: Config,
        progress_manager: &'a ProgressManager,
        verbose: bool,
    ) -> Self {
        // Get the color setting from the progress manager
        Self {
            fs,
            runner,
            config,
            progress_manager,
            verbose,
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
                    "validate",
                    &format!("Validating package '{}'", package_name),
                    ProgressStyleType::Spinner,
                );

                // Create package repository
                let package_repo =
                    YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());

                // Create the enhanced validator
                let validator =
                    PackageValidator::new(self.fs, self.runner, &self.config, &package_repo);

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
                        if self.verbose {
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
