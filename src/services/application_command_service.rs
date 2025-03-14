use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
    domain::{
        application::commands::{ApplicationCommand, ConfigCommand, PackageCommand},
        config::AppConfig,
    },
    ports::{
        application::{ApplicationArguments, ApplicationCommandRouter, ApplicationError},
        command::CommandRunner,
        filesystem::FileSystem,
    },
};

use super::{
    package_installer::{PackageInstaller, PackageInstallerError},
    package_list_service::{PackageListResult, PackageListService},
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
                                        if !output.stderr.is_empty() {
                                            progress_manager
                                                .print_warning("\n\nCommand output (stderr):\n");
                                            progress_manager.print_warning(&output.stderr);
                                        }
                                    }
                                }
                                0
                            }
                            Err(err) => {
                                if let PackageInstallerError::EnhancedError(msg) = &err {
                                    // Print the enhanced error message directly
                                    progress_manager.print_error(msg);
                                } else {
                                    // For other errors, use the standard error formatting
                                    progress_manager
                                        .print_error(format!("Installation failed: {}", err));
                                }
                                1
                            }
                        }
                    }
                    PackageCommand::List => {
                        self.validate_config(false)?;

                        // For list commands, we only need a minimal config validation
                        let package_repo = YamlPackageRepository::new(
                            self.fs,
                            self.app_config.expanded_package_directory(),
                            &progress_manager,
                        );

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
                        // Don't propagate the error; let the command run through even if the
                        // config is bad.
                        let _ = self.validate_config(true);

                        let validate_cmd = ValidationCommand::new(
                            self.fs,
                            &*self.runner,
                            self.app_config,
                            &progress_manager,
                        );

                        match validate_cmd.execute(pkg_cmd) {
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

#[cfg(test)]
mod tests {
    use std::path::{Path, PathBuf};

    use super::*;
    use crate::{
        domain::{application::commands::ApplicationCommand, config::AppConfig},
        ports::{
            application::ApplicationArguments,
            command::MockCommandRunner,
            config_loader::{ConfigLoadError, MockConfigLoader},
            filesystem::MockFileSystem,
        },
    };

    fn setup_service_with_config(
        app_config: Option<AppConfig>,
        app_args: Option<ApplicationArguments>,
    ) -> (MockFileSystem, MockCommandRunner, MockConfigLoader) {
        let fs = MockFileSystem::default();
        let runner = MockCommandRunner::new();
        let mut config_loader = MockConfigLoader::new();

        match (app_config, app_args) {
            (Some(config), Some(app_args)) => {
                config_loader.mock_load_config_ok(app_args, config);
            }
            (None, Some(app_args)) => {
                config_loader.mock_load_config_err(app_args, ConfigLoadError::NotFound);
            }
            (Some(config), None) => {
                config_loader.mock_load_config_ok(ApplicationArguments::default(), config);
            }
            (None, None) => {
                config_loader.mock_load_config_err(
                    ApplicationArguments::default(),
                    ConfigLoadError::NotFound,
                );
            }
        }

        (fs, runner, config_loader)
    }

    // #[test]
    // fn test_build_app_config() {
    //     // Setup mock config loader to return a test config
    //     let app_config = AppConfig::new("file-env".to_string(), PathBuf::from("/file/path"));
    //
    //     // Create arguments with CLI overrides
    //     let args = ApplicationArguments {
    //         environment: Some("cli-env".to_string()),
    //         package_directory: Some(PathBuf::from("/cli/path")),
    //         verbose: true,
    //         no_color: true,
    //         command: ApplicationCommand::Package(PackageCommand::List),
    //     };
    //
    //     let (fs, runner, config_loader) =
    //         setup_service_with_config(Some(app_config.clone()), Some(args.clone()));
    //     let service = ApplicationCommandService::new(&fs, &runner, &app_config);
    //
    //     // Build the app config
    //     let app_config = service.build_config(&args, false).unwrap();
    //
    //     // CLI args should take precedence
    //     assert_eq!(app_config.environment(), "cli-env");
    //     assert_eq!(app_config.package_directory(), Path::new("/cli/path"));
    //     assert!(app_config.verbose());
    //     assert!(!app_config.use_colors());
    //
    //     // But other settings should come from file config
    //     assert_eq!(app_config.command_timeout().as_secs(), 60); // Default value
    // }

    // #[test]
    // fn test_build_app_config_no_file() {
    //     // Setup default config
    //     let default_config =
    //         AppConfig::new("default-env".to_string(), PathBuf::from("/default/path"));
    //     let (fs, runner, config_loader) = setup_service_with_config(Some(default_config), None);
    //
    //     let service = ApplicationCommandService::new(&fs, &runner, &config_loader);
    //
    //     // Create arguments with no overrides
    //     let args = ApplicationArguments {
    //         environment: None,
    //         package_directory: None,
    //         verbose: false,
    //         no_color: false,
    //         command: ApplicationCommand::Package(PackageCommand::List),
    //     };
    //
    //     // Build the app config
    //     let app_config = service.build_config(&args, false).unwrap();
    //
    //     // Should use default config
    //     assert_eq!(app_config.environment(), "default-env");
    //     assert_eq!(app_config.package_directory(), Path::new("/default/path"));
    //     assert!(!app_config.verbose());
    //     assert!(app_config.use_colors());
    // }
}
