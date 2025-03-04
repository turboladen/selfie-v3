// src/services/package_validation_service.rs
// Implements the 'selfie package validate' command with enhanced command validation

use crate::{
    adapters::package_repo::yaml::YamlPackageRepository,
    adapters::progress::{ProgressManager, ProgressStyleType},
    cli::PackageSubcommands,
    domain::config::Config,
    ports::{command::CommandRunner, filesystem::FileSystem},
    services::{
        command_validator::CommandValidator,
        package_validator::{
            format_validation_result, PackageValidator, PackageValidatorError,
            ValidationErrorCategory, ValidationIssue,
        },
    },
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

/// Handles the 'package validate' command with enhanced command validation
pub struct PackageValidationService<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    runner: &'a R, // Changed from _runner to runner to indicate it's now used
    config: Config,
    progress_manager: &'a ProgressManager,
    verbose: bool,
    use_colors: bool,
}

impl<'a, F: FileSystem, R: CommandRunner> PackageValidationService<'a, F, R> {
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
    pub fn execute(&self, cmd: &PackageSubcommands) -> PackageValidationResult {
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

                let package_repo =
                    YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());

                // Create validator
                let validator = PackageValidator::new(self.fs, &self.config, &package_repo);

                // Create command validator using our runner
                let command_validator = CommandValidator::new(self.runner);

                // Validate package
                let result = if let Some(path) = package_path {
                    validator.validate_package_file(path)
                } else {
                    validator.validate_package(package_name)
                };

                match result {
                    Ok(mut validation_result) => {
                        // If we have a valid package, enhance validation with command checks
                        if let Some(package) = &validation_result.package {
                            // First collect all validation issues to avoid mutably borrowing
                            // validation_result while holding an immutable reference to package
                            let mut command_issues = Vec::new();
                            let mut terminal_warnings = Vec::new();

                            // Add command validation for each environment
                            for (env_name, env_config) in &package.environments {
                                let command_results = command_validator
                                    .validate_environment_commands(env_name, env_config);

                                // Process command validation results
                                for cmd_result in command_results {
                                    if !cmd_result.is_valid
                                        || (cmd_result.is_warning && self.verbose)
                                    {
                                        // Only add issues for invalid commands and warnings in verbose mode
                                        if let Some(ref error) = cmd_result.error {
                                            let field_name = format!(
                                                "environments.{}.{}",
                                                env_name,
                                                if cmd_result.command == env_config.install {
                                                    "install"
                                                } else if let Some(check) = &env_config.check {
                                                    if &cmd_result.command == check {
                                                        "check"
                                                    } else {
                                                        "command"
                                                    }
                                                } else {
                                                    "command"
                                                }
                                            );

                                            if cmd_result.is_warning {
                                                command_issues.push(ValidationIssue::warning(
                                                    ValidationErrorCategory::CommandSyntax,
                                                    &field_name,
                                                    error,
                                                    None,
                                                    Some("Consider updating the command for better portability and safety."),
                                                ));
                                            } else {
                                                command_issues.push(ValidationIssue::error(
                                                    ValidationErrorCategory::CommandSyntax,
                                                    &field_name,
                                                    error,
                                                    None,
                                                    Some("Fix the command syntax before using this package."),
                                                ));
                                            }
                                        }
                                    }

                                    // Availability check
                                    if !cmd_result.is_available && cmd_result.is_warning {
                                        // This is a warning about command not being available
                                        terminal_warnings.push(format!(
                                            "Warning: {} for environment '{}'",
                                            cmd_result
                                                .error
                                                .unwrap_or_else(|| "Command not found".to_string()),
                                            cmd_result.environment
                                        ));
                                    }
                                }

                                // Special checks for potentially problematic commands
                                if command_validator.might_require_sudo(&env_config.install) {
                                    command_issues.push(ValidationIssue::warning(
                                        ValidationErrorCategory::CommandSyntax,
                                        &format!("environments.{}.install", env_name),
                                        "Command might require sudo privileges",
                                        None,
                                        Some("This command may require administrative privileges to run."),
                                    ));
                                }

                                if command_validator.uses_backticks(&env_config.install) {
                                    command_issues.push(ValidationIssue::warning(
                                        ValidationErrorCategory::CommandSyntax,
                                        &format!("environments.{}.install", env_name),
                                        "Command uses backticks for command substitution",
                                        None,
                                        Some("Consider using $() instead of backticks for better nesting and readability."),
                                    ));
                                }

                                if command_validator.might_download_content(&env_config.install) {
                                    command_issues.push(ValidationIssue::warning(
                                        ValidationErrorCategory::CommandSyntax,
                                        &format!("environments.{}.install", env_name),
                                        "Command may download content from the internet",
                                        None,
                                        Some("This command appears to download content, which may pose security risks."),
                                    ));
                                }

                                // Add environment-specific recommendations
                                if let Some(recommendation) = command_validator
                                    .is_command_recommended_for_env(env_name, &env_config.install)
                                {
                                    command_issues.push(ValidationIssue::warning(
                                        ValidationErrorCategory::Environment,
                                        &format!("environments.{}.install", env_name),
                                        &recommendation,
                                        None,
                                        Some("Using environment-specific package managers may improve reliability."),
                                    ));
                                }
                            }

                            // Now add all collected issues to the validation result
                            for issue in command_issues {
                                validation_result.add_issue(issue);
                            }

                            // Print any terminal warnings
                            for warning in terminal_warnings {
                                progress.println(warning);
                            }
                        }

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
                            PackageValidatorError::PackageNotFound(name) => {
                                PackageValidationResult::Error(format!(
                                    "Package '{}' not found\n\nVerify the package name and make sure the package file exists in the package directory.",
                                    name
                                ))
                            }
                            PackageValidatorError::MultiplePackagesFound(name) => {
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
