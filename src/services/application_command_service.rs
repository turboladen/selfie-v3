use crate::{
    adapters::{
        package_repo::yaml::YamlPackageRepository,
        progress::{ProgressManager, ProgressStyleType},
    },
    domain::{
        application::commands::{ApplicationCommand, ConfigCommand, PackageCommand},
        config::Config,
    },
    ports::{
        application::{ApplicationArguments, ApplicationCommandRouter, ApplicationError},
        command::CommandRunner,
        config_loader::{ConfigLoadError, ConfigLoader},
        filesystem::FileSystem,
    },
};

use super::{
    package_installer::PackageInstaller,
    package_list_service::{PackageListResult, PackageListService},
    validation_command::{ValidationCommand, ValidationCommandResult},
};

pub struct ApplicationCommandService<'a, F: FileSystem, R: CommandRunner, C: ConfigLoader> {
    fs: &'a F,
    runner: &'a R,
    config_loader: &'a C,
}

impl<'a, F: FileSystem, R: CommandRunner, C: ConfigLoader> ApplicationCommandService<'a, F, R, C> {
    pub fn new(fs: &'a F, runner: &'a R, config_loader: &'a C) -> Self {
        Self {
            fs,
            runner,
            config_loader,
        }
    }

    // Updated to use the config loader
    fn build_config(
        &self,
        args: &ApplicationArguments,
        full_validation: bool,
    ) -> Result<Config, ApplicationError> {
        // Try to load base config from file
        let base_config = match self.config_loader.load_config() {
            Ok(config) => Some(config),
            Err(err) => {
                // Log the error but continue with default config if we can
                if let ConfigLoadError::NotFound = err {
                    None // Not found is fine, we'll use defaults
                } else {
                    // For other errors, log but continue with defaults
                    eprintln!("Warning: Failed to load config file: {}", err);
                    None
                }
            }
        };

        // Start with either the loaded config or default
        let mut config = base_config.unwrap_or_else(|| self.config_loader.default_config());

        // Override with CLI arguments
        if let Some(env) = &args.environment {
            config = Config::new(env.clone(), config.package_directory.clone());
        }

        if let Some(pkg_dir) = &args.package_directory {
            config = Config::new(config.environment.clone(), pkg_dir.clone());
        }

        // Validate based on requirements
        if full_validation {
            config.validate().map_err(ApplicationError::ConfigError)?;
        } else {
            config
                .validate_minimal()
                .map_err(ApplicationError::ConfigError)?;
        }

        Ok(config)
    }
}

impl<F: FileSystem, R: CommandRunner, C: ConfigLoader> ApplicationCommandRouter
    for ApplicationCommandService<'_, F, R, C>
{
    fn process_command(&self, args: ApplicationArguments) -> Result<i32, ApplicationError> {
        // Create a progress manager using the arguments
        let progress_manager = ProgressManager::new(!args.no_color, true, args.verbose);

        // Display the command description
        let cmd_desc = self.get_command_description(&args.command);
        let info_pb =
            progress_manager.create_progress_bar("info", &cmd_desc, ProgressStyleType::Message);
        info_pb.finish();

        let exit_code = match &args.command {
            ApplicationCommand::Package(pkg_cmd) => {
                match &pkg_cmd {
                    PackageCommand::Install { package_name } => {
                        // For install commands, we need a fully valid config
                        match self.build_config(&args, true) {
                            Ok(config) => {
                                // Use the consolidated package installer
                                let installer = PackageInstaller::new(
                                    self.fs,
                                    self.runner,
                                    &config,
                                    &progress_manager,
                                    true, // Enable command checking
                                );

                                match installer.install_package(package_name) {
                                    Ok(_) => 0,
                                    Err(err) => {
                                        let error_pb = progress_manager.create_progress_bar(
                                            "error",
                                            &format!("Installation failed: {}", err),
                                            ProgressStyleType::Message,
                                        );
                                        error_pb.abandon();
                                        1
                                    }
                                }
                            }
                            Err(err) => {
                                // The progress already showed a failure indicator
                                // Just print the error details
                                eprintln!("Error: {}", err);
                                1
                            }
                        }
                    }
                    PackageCommand::List => {
                        // For list commands, we only need a minimal config validation
                        match self.build_config(&args, false) {
                            Ok(config) => {
                                let package_repo = YamlPackageRepository::new(
                                    self.fs,
                                    config.expanded_package_directory(),
                                );
                                let list_cmd = PackageListService::new(
                                    self.runner,
                                    &config,
                                    &progress_manager,
                                    args.verbose,
                                    &package_repo,
                                );

                                match list_cmd.execute() {
                                    PackageListResult::Success(output) => {
                                        // The progress bar already showed "Done"
                                        // Just print the package list
                                        println!("{}", output);
                                        0
                                    }
                                    PackageListResult::Error(error) => {
                                        // The progress bar already showed "Failed"
                                        // Print the detailed error
                                        eprintln!("{}", error);
                                        1
                                    }
                                }
                            }
                            Err(err) => {
                                eprintln!("Error: {}", err);
                                eprintln!("\nPackage directory is required for listing packages.");
                                eprintln!("You can set it with:");
                                eprintln!("  1. The --package-directory flag: --package-directory /path/to/packages");
                                eprintln!("  2. In your config.yaml file: package_directory: /path/to/packages");
                                1
                            }
                        }
                    }
                    PackageCommand::Info { package_name } => {
                        let info_pb = progress_manager.create_progress_bar(
                            "info",
                            &format!("Package info for '{}' not implemented yet", package_name),
                            ProgressStyleType::Message,
                        );
                        info_pb.finish();
                        0
                    }
                    PackageCommand::Create { package_name } => {
                        let info_pb = progress_manager.create_progress_bar(
                            "create",
                            &format!(
                                "Package creation for '{}' not implemented yet",
                                package_name
                            ),
                            ProgressStyleType::Message,
                        );
                        info_pb.finish();
                        0
                    }
                    PackageCommand::Validate { .. } => {
                        // For validation commands, use minimal config validation that only checks package directory
                        match self.build_config(&args, true) {
                            Ok(config) => {
                                // Use the consolidated validate command
                                let validate_cmd = ValidationCommand::new(
                                    self.fs,
                                    self.runner,
                                    config,
                                    &progress_manager,
                                    args.verbose,
                                );

                                match validate_cmd.execute(pkg_cmd) {
                                    ValidationCommandResult::Valid(output) => {
                                        // The progress bar already showed "Validation successful"
                                        // Just print the details now
                                        println!("{}", output);
                                        0
                                    }
                                    ValidationCommandResult::Invalid(output) => {
                                        // The progress bar already showed "Validation failed"
                                        // Just print the details now
                                        println!("{}", output);
                                        1
                                    }
                                    ValidationCommandResult::Error(error) => {
                                        // The progress bar already showed a generic failure message
                                        // Print the detailed error to stderr
                                        eprintln!("{}", error);
                                        1
                                    }
                                }
                            }
                            Err(err) => {
                                eprintln!("Error: {}", err);
                                eprintln!("\nPackage directory is required for validation.");
                                eprintln!("You can set it with:");
                                eprintln!("  1. The --package-directory flag: --package-directory /path/to/packages");
                                eprintln!("  2. In your config.yaml file: package_directory: /path/to/packages");
                                1
                            }
                        }
                    }
                }
            }
            ApplicationCommand::Config(_cfg_cmd) => {
                let info_pb = progress_manager.create_progress_bar(
                    "config",
                    "Config commands not implemented yet",
                    ProgressStyleType::Message,
                );
                info_pb.finish();
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
