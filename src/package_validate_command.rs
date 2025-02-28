// src/package_validate_command.rs

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
    use_colors: bool, // Added field to track color setting
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
        // Get the color setting from the progress manager
        let use_colors = progress_manager.use_colors();
        Self {
            fs,
            _runner: runner,
            config,
            progress_manager,
            verbose,
            use_colors,
        }
    }

    /// Execute the validate command
    pub async fn execute(&self, cmd: &PackageSubcommands) -> ValidateCommandResult {
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
                    validator.validate_package_file(path).await
                } else {
                    validator.validate_package(package_name).await
                };

                match result {
                    Ok(validation_result) => {
                        // Format validation result with color support and verbose flag
                        let formatted = format_validation_result(
                            &validation_result,
                            self.use_colors,
                            self.verbose, // Pass verbose flag to control detail level
                        );

                        // Additional verbose info - show package path and structure
                        if self.verbose && validation_result.package.is_some() {
                            // Add more detailed package information when in verbose mode
                            progress.println("Detailed package structure:");
                            if let Some(package) = &validation_result.package {
                                // Show more details about environments and configurations
                                progress.println(format!(
                                    "  Environments: {}",
                                    package
                                        .environments
                                        .keys()
                                        .map(|k| k.to_string())
                                        .collect::<Vec<_>>()
                                        .join(", ")
                                ));
                                // Could add more verbose details here
                            }
                        }

                        if validation_result.is_valid() {
                            progress.finish_with_message("Validation successful");
                            ValidateCommandResult::Valid(formatted)
                        } else {
                            progress.abandon_with_message("Validation failed");
                            ValidateCommandResult::Invalid(formatted)
                        }
                    }
                    Err(err) => {
                        // More verbose error handling
                        if self.verbose {
                            progress.println(format!("Error details: {:#?}", err));
                        }

                        progress.abandon_with_message("Validation failed");

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
