use std::path::Path;

use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
    domain::{
        application::commands::{ApplicationCommand, ConfigCommand, PackageCommand},
        config::AppConfig,
    },
    ports::{
        application::{ApplicationArguments, ApplicationCommandRouter, ApplicationError},
        command::CommandRunner,
        filesystem::{FileSystem, FileSystemError},
    },
};

use super::{
    command_validator::CommandValidator,
    enhanced_error_handler::EnhancedErrorHandler,
    package::{
        install::{PackageInstaller, PackageInstallerError},
        list::{PackageListResult, PackageListService},
    },
    validation_command::{ValidationCommand, ValidationCommandResult},
};

pub struct ApplicationCommandService<'a> {
    fs: &'a dyn FileSystem,
    runner: Box<dyn CommandRunner>,
    app_config: &'a AppConfig,
}

impl<'a> ApplicationCommandService<'a> {
    pub fn new(
        fs: &'a dyn FileSystem,
        runner: Box<dyn CommandRunner>,
        app_config: &'a AppConfig,
    ) -> Self {
        Self {
            fs,
            runner,
            app_config,
        }
    }

    // Updated to use the config loader
    fn validate_config(&self, full_validation: bool) -> Result<(), ApplicationError> {
        // Validate based on requirements
        if full_validation {
            self.app_config
                .validate()
                .map_err(ApplicationError::ConfigError)?;
        } else {
            self.app_config
                .validate_minimal()
                .map_err(ApplicationError::ConfigError)?;
        }

        Ok(())
    }
}

#[async_trait::async_trait]
impl ApplicationCommandRouter for ApplicationCommandService<'_> {
    async fn process_command(&self, args: ApplicationArguments) -> Result<i32, ApplicationError> {
        // Create a progress manager using the unified AppConfig
        let progress_manager = ProgressManager::from(self.app_config);

        // Display the command description
        let cmd_desc = self.get_command_description(&args.command);
        progress_manager.info(&cmd_desc);

        let exit_code = match &args.command {
            ApplicationCommand::Package(pkg_cmd) => {
                // Create error handler for better error presentation
                let package_repo = YamlPackageRepository::new(
                    self.fs,
                    self.app_config.expanded_package_directory(),
                    &progress_manager,
                );
                let error_handler =
                    EnhancedErrorHandler::new(self.fs, &package_repo, &progress_manager);

                match &pkg_cmd {
                    PackageCommand::Install { package_name } => {
                        self.validate_config(true)?;

                        // For install commands, we need a fully valid config
                        // Use the consolidated package installer with our unified config
                        let installer = PackageInstaller::new(
                            self.fs,
                            &*self.runner,
                            self.app_config,
                            &progress_manager,
                            true, // Enable command checking
                        );

                        match installer.install_package(package_name).await {
                            Ok(result) => {
                                if self.app_config.verbose() {
                                    if let Some(output) = &result.command_output {
                                        if !output.stdout.is_empty() {
                                            progress_manager
                                                .print_info("\n\nCommand output (stdout):\n");
                                            progress_manager.print_progress(&output.stdout);
                                        }
                                        if !output.stderr.is_empty() {
                                            progress_manager
                                                .print_warning("\n\nCommand output (stderr):\n");
                                            progress_manager.print_progress(&output.stderr);
                                        }
                                    }
                                }
                                0
                            }
                            Err(err) => {
                                // Check for filesystem errors specifically
                                match &err {
                                    PackageInstallerError::FileSystemError(fs_err) => {
                                        if let FileSystemError::PathNotFound(path_str) = fs_err {
                                            let error_msg = error_handler
                                                .handle_path_not_found(Path::new(path_str));
                                            progress_manager.print_error(&error_msg);
                                        }
                                    }
                                    PackageInstallerError::EnhancedError(msg) => {
                                        // Print the enhanced error message directly
                                        progress_manager.print_error(msg);
                                    }
                                    // Handle other error variants as needed
                                    _ => {
                                        progress_manager
                                            .print_error(format!("Installation failed: {}", err));
                                    }
                                }
                                1
                            }
                        }
                    }
                    PackageCommand::List => {
                        self.validate_config(false)?;

                        let list_cmd = PackageListService::new(
                            &*self.runner,
                            self.app_config,
                            &progress_manager,
                            &package_repo,
                        );

                        match list_cmd.execute().await {
                            PackageListResult::Success(output) => {
                                // Just print the package list
                                progress_manager.print_progress(output);
                                0
                            }
                            PackageListResult::Error(error) => {
                                // Print the detailed error
                                progress_manager.print_error(error);
                                1
                            }
                        }
                    }
                    PackageCommand::Info { package_name } => {
                        self.validate_config(false)?;

                        progress_manager.print_warning(format!(
                            "Package info for '{}' not implemented yet",
                            package_name
                        ));
                        0
                    }
                    PackageCommand::Create { package_name } => {
                        self.validate_config(true)?;

                        progress_manager.print_warning(format!(
                            "Package creation for '{}' not implemented yet",
                            package_name
                        ));
                        0
                    }
                    PackageCommand::Validate { .. } => {
                        // Don't propagate the error; let the ?command run through even if the
                        // config is bad.
                        let _ = self.validate_config(true);
                        let command_validator = CommandValidator::new(&*self.runner);

                        let validate_cmd = ValidationCommand::new(
                            self.fs,
                            self.app_config,
                            &progress_manager,
                            &command_validator,
                        );

                        match validate_cmd.execute(pkg_cmd).await {
                            ValidationCommandResult::Valid(output) => {
                                progress_manager.print_success(output);
                                0
                            }
                            ValidationCommandResult::Invalid(output) => {
                                progress_manager.print_warning(output);
                                1
                            }
                            ValidationCommandResult::Error(error) => {
                                progress_manager.print_error(error);
                                1
                            }
                        }
                    }
                }
            }
            ApplicationCommand::Config(_cfg_cmd) => {
                progress_manager.info("Config commands not implemented yet");
                0
            }
        };

        Ok(exit_code)
    }

    fn get_command_description(&self, command: &ApplicationCommand) -> String {
        match command {
            ApplicationCommand::Package(pkg_cmd) => match pkg_cmd {
                PackageCommand::Install { package_name } => {
                    format!("Install package '{}'", package_name)
                }
                PackageCommand::List => "List available packages".to_string(),
                PackageCommand::Info { package_name } => {
                    format!("Show information about package '{}'", package_name)
                }
                PackageCommand::Create { package_name } => {
                    format!("Create package '{}'", package_name)
                }
                PackageCommand::Validate {
                    package_name,
                    package_path,
                } => match package_path {
                    Some(path) => {
                        format!("Validate package '{}' ({})", package_name, path.display())
                    }
                    None => format!("Validate package '{}'", package_name),
                },
            },
            ApplicationCommand::Config(cfg_cmd) => match cfg_cmd {
                ConfigCommand::Validate => "Validate configuration".to_string(),
            },
        }
    }
}
