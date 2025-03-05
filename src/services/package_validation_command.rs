// src/services/package_validation_command.rs
use crate::{
    adapters::{
        package_repo::yaml::YamlPackageRepository,
        progress::{ProgressManager, ProgressStyleType},
    },
    domain::{application::commands::PackageCommand, config::Config, validation::ValidationError},
    ports::{command::CommandRunner, filesystem::FileSystem},
    services::validation_service::ValidationService,
};

/// Result of running the validate command
pub enum PackageValidationResult {
    /// Package validation successful (may include warnings)
    Valid(String),
    /// Package validation failed with errors
    Invalid(String),
    /// Command failed to run
    Error(String),
}

/// Handles the 'package validate' command
pub struct PackageValidationCommand<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    runner: &'a R,
    config: Config,
    progress_manager: &'a ProgressManager,
    verbose: bool,
    use_colors: bool,
}

impl<'a, F: FileSystem, R: CommandRunner> PackageValidationCommand<'a, F, R> {
    /// Create a new validate command handler
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: Config,
        progress_manager: &'a ProgressManager,
        verbose: bool,
    ) -> Self {
        // Get the color setting from the progress manager
        let use_colors = progress_manager.use_colors();
        Self {
            fs,
            runner,
            config,
            progress_manager,
            verbose,
            use_colors,
        }
    }

    /// Execute the validate command
    pub fn execute(&self, cmd: &PackageCommand) -> PackageValidationResult {
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

                // Create validation service
                let validation_service =
                    ValidationService::new(self.fs, self.runner, &self.config, &package_repo);

                // Validate package
                let result = if let Some(path) = package_path {
                    validation_service.validate_package_file(path)
                } else {
                    validation_service.validate_package_by_name(package_name)
                };

                match result {
                    Ok(validation_result) => {
                        // Format the validation result
                        let formatted = validation_service.format_validation_result(
                            &validation_result,
                            self.use_colors,
                            self.verbose,
                        );

                        if validation_result.is_valid() {
                            progress.finish_with_message("Validation successful");
                            PackageValidationResult::Valid(formatted)
                        } else {
                            progress.abandon_with_message("Validation failed");
                            PackageValidationResult::Invalid(formatted)
                        }
                    }
                    Err(err) => {
                        // More verbose error handling
                        if self.verbose {
                            progress.println(format!("Error details: {:#?}", err));
                        }

                        progress.abandon_with_message("Validation failed");

                        match err {
                            ValidationError::PackageNotFound(name) => {
                                PackageValidationResult::Error(format!(
                                    "Package '{}' not found\n\nVerify the package name and make sure the package file exists in the package directory.",
                                    name
                                ))
                            }
                            ValidationError::MultiplePackagesFound(name) => {
                                PackageValidationResult::Error(format!(
                                    "Multiple package files found for '{}'\n\nUse the --package-path flag to specify which file to validate.",
                                    name
                                ))
                            }
                            _ => PackageValidationResult::Error(format!("Error: {}", err)),
                        }
                    }
                }
            }
            _ => PackageValidationResult::Error(
                "Invalid command. Expected 'validate <package-name>'".to_string(),
            ),
        }
    }
}
