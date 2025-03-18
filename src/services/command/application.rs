use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
    domain::{
        application::commands::{ApplicationCommand, ConfigCommand, PackageCommand},
        config::AppConfig,
    },
    ports::{
        application::{ApplicationArguments, ApplicationCommandRouter},
        command::CommandRunner,
        filesystem::FileSystem,
    },
    services::enhanced_error_handler::EnhancedErrorHandler,
};

use super::package::PackageCommandService;

pub struct ApplicationCommandService<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    runner: R,
    app_config: &'a AppConfig,
}

impl<'a, F: FileSystem, R: CommandRunner> ApplicationCommandService<'a, F, R> {
    pub fn new(fs: &'a F, runner: R, app_config: &'a AppConfig) -> Self {
        Self {
            fs,
            runner,
            app_config,
        }
    }
}

#[async_trait::async_trait]
impl<F: FileSystem, R: CommandRunner> ApplicationCommandRouter
    for ApplicationCommandService<'_, F, R>
{
    async fn process_command(&self, args: ApplicationArguments) -> Result<i32, anyhow::Error> {
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
                let package_command_service = PackageCommandService::new(
                    self.fs,
                    &self.runner,
                    &package_repo,
                    &progress_manager,
                    self.app_config,
                );
                let error_handler =
                    EnhancedErrorHandler::new(self.fs, &package_repo, &progress_manager);

                match &pkg_cmd {
                    PackageCommand::Install { package_name } => {
                        package_command_service
                            .install(package_name, &error_handler)
                            .await?
                    }
                    PackageCommand::List => package_command_service.list().await?,
                    PackageCommand::Info { package_name } => {
                        package_command_service.info(package_name)?
                    }
                    PackageCommand::Create { package_name } => {
                        package_command_service.create(package_name)?
                    }
                    PackageCommand::Validate {
                        package_name,
                        package_path,
                    } => {
                        package_command_service
                            .validate(package_name, package_path.as_deref())
                            .await
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
