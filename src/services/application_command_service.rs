use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
    domain::{
        application::commands::{ApplicationCommand, ConfigCommand, PackageCommand},
        config::AppConfig,
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
    ) -> Result<AppConfig, ApplicationError> {
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
        let file_config = base_config.unwrap_or_else(|| self.config_loader.default_config());

        // Create AppConfig and apply CLI overrides
        let app_config = AppConfig::from_file_config(file_config).apply_cli_args(
            args.environment.clone(),
            args.package_directory.clone(),
            args.verbose,
            args.no_color,
        );

        // Validate based on requirements
        if full_validation {
            app_config
                .validate()
                .map_err(ApplicationError::ConfigError)?;
        } else {
            app_config
                .validate_minimal()
                .map_err(ApplicationError::ConfigError)?;
        }

        Ok(app_config)
    }
}

impl<F: FileSystem, R: CommandRunner, C: ConfigLoader> ApplicationCommandRouter
    for ApplicationCommandService<'_, F, R, C>
{
    fn process_command(&self, args: ApplicationArguments) -> Result<i32, ApplicationError> {
        // Build the AppConfig first - this consolidates settings from config file and CLI args
        let app_config = self.build_config(&args, true)?;

        // Create a progress manager using the unified AppConfig
        let progress_manager = ProgressManager::from(&app_config);

        // Display the command description
        let cmd_desc = self.get_command_description(&args.command);
        progress_manager.info(&cmd_desc);

        let exit_code = match &args.command {
            ApplicationCommand::Package(pkg_cmd) => {
                match &pkg_cmd {
                    PackageCommand::Install { package_name } => {
                        // For install commands, we need a fully valid config
                        // Use the consolidated package installer with our unified config
                        let installer = PackageInstaller::new(
                            self.fs,
                            self.runner,
                            &app_config,
                            &progress_manager,
                            true, // Enable command checking
                        );

                        match installer.install_package(package_name) {
                            Ok(_) => 0,
                            Err(err) => {
                                progress_manager.error(&format!("Installation failed: {}", err));
                                1
                            }
                        }
                    }
                    PackageCommand::List => {
                        // For list commands, we only need a minimal config validation
                        let package_repo = YamlPackageRepository::new(
                            self.fs,
                            app_config.expanded_package_directory(),
                        );

                        let list_cmd = PackageListService::new(
                            self.runner,
                            &app_config,
                            &progress_manager,
                            &package_repo,
                        );

                        match list_cmd.execute() {
                            PackageListResult::Success(output) => {
                                // Just print the package list
                                println!("{}", output);
                                0
                            }
                            PackageListResult::Error(error) => {
                                // Print the detailed error
                                eprintln!("{}", error);
                                1
                            }
                        }
                    }
                    PackageCommand::Info { package_name } => {
                        progress_manager.info(&format!(
                            "Package info for '{}' not implemented yet",
                            package_name
                        ));
                        0
                    }
                    PackageCommand::Create { package_name } => {
                        progress_manager.info(&format!(
                            "Package creation for '{}' not implemented yet",
                            package_name
                        ));
                        0
                    }
                    PackageCommand::Validate { .. } => {
                        // For validation commands, use minimal config validation

                        // Use the consolidated validate command with unified config
                        let validate_cmd = ValidationCommand::new(
                            self.fs,
                            self.runner,
                            &app_config,
                            &progress_manager,
                        );

                        match validate_cmd.execute(pkg_cmd) {
                            ValidationCommandResult::Valid(output) => {
                                println!("{}", output);
                                0
                            }
                            ValidationCommandResult::Invalid(output) => {
                                println!("{}", output);
                                1
                            }
                            ValidationCommandResult::Error(error) => {
                                eprintln!("{}", error);
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
        domain::{application::commands::ApplicationCommand, config::FileConfig},
        ports::{
            application::ApplicationArguments, command::MockCommandRunner,
            config_loader::MockConfigLoader, filesystem::MockFileSystem,
        },
    };

    fn setup_service_with_config(
        file_config: Option<FileConfig>,
        default_config: Option<FileConfig>,
    ) -> (MockFileSystem, MockCommandRunner, MockConfigLoader) {
        let fs = MockFileSystem::default();
        let runner = MockCommandRunner::new();
        let mut config_loader = MockConfigLoader::new();

        if let Some(config) = file_config {
            config_loader
                .expect_load_config()
                .returning(move || Ok(config.clone()));
        } else {
            config_loader
                .expect_load_config()
                .returning(|| Err(ConfigLoadError::NotFound));
        }

        if let Some(config) = default_config {
            config_loader
                .expect_default_config()
                .returning(move || config.clone());
        }

        (fs, runner, config_loader)
    }

    #[test]
    fn test_build_app_config() {
        // Setup mock config loader to return a test config
        let file_config = FileConfig::new("file-env".to_string(), PathBuf::from("/file/path"));
        let (fs, runner, config_loader) = setup_service_with_config(Some(file_config), None);
        let service = ApplicationCommandService::new(&fs, &runner, &config_loader);

        // Create arguments with CLI overrides
        let args = ApplicationArguments {
            environment: Some("cli-env".to_string()),
            package_directory: Some(PathBuf::from("/cli/path")),
            verbose: true,
            no_color: true,
            command: ApplicationCommand::Package(PackageCommand::List),
        };

        // Build the app config
        let app_config = service.build_config(&args, false).unwrap();

        // CLI args should take precedence
        assert_eq!(app_config.environment(), "cli-env");
        assert_eq!(app_config.package_directory(), Path::new("/cli/path"));
        assert!(app_config.verbose());
        assert!(!app_config.use_colors());

        // But other settings should come from file config
        assert_eq!(app_config.command_timeout().as_secs(), 60); // Default value
    }

    #[test]
    fn test_build_app_config_no_file() {
        // Setup default config
        let default_config =
            FileConfig::new("default-env".to_string(), PathBuf::from("/default/path"));
        let (fs, runner, config_loader) = setup_service_with_config(Some(default_config), None);

        let service = ApplicationCommandService::new(&fs, &runner, &config_loader);

        // Create arguments with no overrides
        let args = ApplicationArguments {
            environment: None,
            package_directory: None,
            verbose: false,
            no_color: false,
            command: ApplicationCommand::Package(PackageCommand::List),
        };

        // Build the app config
        let app_config = service.build_config(&args, false).unwrap();

        // Should use default config
        assert_eq!(app_config.environment(), "default-env");
        assert_eq!(app_config.package_directory(), Path::new("/default/path"));
        assert!(!app_config.verbose());
        assert!(app_config.use_colors());
    }
}
