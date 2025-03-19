use std::path::Path;

use thiserror::Error;

use crate::{
    adapters::progress::ProgressManager,
    domain::config::{AppConfig, ConfigValidationError},
    ports::{
        command::CommandRunner,
        filesystem::{FileSystem, FileSystemError},
        package_repo::PackageRepository,
    },
    services::{
        enhanced_error_handler::EnhancedErrorHandler,
        package::{
            install::{PackageInstaller, PackageInstallerError},
            list::{PackageListResult, PackageListService},
        },
    },
};

#[derive(Debug, Error)]
pub(super) enum PackageInstallCommandError {
    #[error(transparent)]
    ConfigError(#[from] ConfigValidationError),
}

#[derive(Debug, Error)]
pub(super) enum PackageListCommandError {
    #[error(transparent)]
    ConfigError(#[from] ConfigValidationError),
}

pub(super) struct PackageCommandService<'a, F: FileSystem, CR: CommandRunner, PR: PackageRepository>
{
    fs: &'a F,
    runner: &'a CR,
    package_repo: &'a PR,
    progress_manager: ProgressManager,
    app_config: &'a AppConfig,
}

impl<'a, F: FileSystem, CR: CommandRunner, PR: PackageRepository>
    PackageCommandService<'a, F, CR, PR>
{
    pub(super) fn new(
        fs: &'a F,
        runner: &'a CR,
        package_repo: &'a PR,
        progress_manager: ProgressManager,
        app_config: &'a AppConfig,
    ) -> Self {
        Self {
            fs,
            runner,
            package_repo,
            progress_manager,
            app_config,
        }
    }

    pub(super) async fn install(
        &self,
        package_name: &str,
        error_handler: &EnhancedErrorHandler<'_>,
    ) -> Result<i32, PackageInstallCommandError> {
        self.app_config.validate()?;

        // For install commands, we need a fully valid config
        // Use the consolidated package installer with our unified config
        let installer = PackageInstaller::new(
            self.package_repo,
            error_handler,
            self.runner,
            self.app_config,
            self.progress_manager,
            true, // Enable command checking
        );

        match installer.install_package(package_name).await {
            Ok(_) => Ok(0),
            Err(err) => {
                // Check for filesystem errors specifically
                match &err {
                    PackageInstallerError::FileSystemError(fs_err) => {
                        if let FileSystemError::PathNotFound(path_str) = fs_err {
                            let error_msg =
                                error_handler.handle_path_not_found(Path::new(path_str));
                            self.progress_manager.print_error(&error_msg);
                        }
                    }
                    PackageInstallerError::EnhancedError(msg) => {
                        // Print the enhanced error message directly
                        self.progress_manager.print_error(msg);
                    }
                    // Handle other error variants as needed
                    _ => {
                        self.progress_manager
                            .print_error(format!("Installation failed: {}", err));
                    }
                }
                Ok(1)
            }
        }
    }

    pub(super) async fn list(&self) -> Result<i32, PackageListCommandError> {
        self.app_config.validate_minimal()?;

        let list_cmd = PackageListService::new(
            self.runner,
            self.app_config,
            self.progress_manager,
            self.package_repo,
        );

        match list_cmd.execute().await {
            PackageListResult::Success(output) => {
                // Just print the package list
                self.progress_manager.print_progress(output);
                Ok(0)
            }
            PackageListResult::Error(error) => {
                // Print the detailed error
                self.progress_manager.print_error(error);
                Ok(1)
            }
        }
    }

    pub(super) fn info(&self, package_name: &str) -> Result<i32, anyhow::Error> {
        self.app_config.validate_minimal()?;

        self.progress_manager.print_warning(format!(
            "Package info for '{}' not implemented yet",
            package_name
        ));
        Ok(0)
    }

    pub(super) fn create(&self, package_name: &str) -> Result<i32, anyhow::Error> {
        self.app_config.validate()?;

        self.progress_manager.print_warning(format!(
            "Package creation for '{}' not implemented yet",
            package_name
        ));
        Ok(0)
    }

    pub(super) async fn validate(&self, package_name: &str, package_path: Option<&Path>) -> i32 {
        use crate::services::{
            command_validator::CommandValidator,
            validation_command::{ValidationCommand, ValidationCommandResult},
        };

        // Don't propagate the error; let the ?command run through even if the
        // config is bad.
        let _ = self.app_config.validate();
        let command_validator = CommandValidator::new(self.runner);

        let validate_cmd = ValidationCommand::new(
            self.fs,
            self.app_config,
            self.progress_manager,
            &command_validator,
        );

        match validate_cmd.execute(package_name, package_path).await {
            ValidationCommandResult::Valid(output) => {
                self.progress_manager.print_success(output);
                0
            }
            ValidationCommandResult::Invalid(output) => {
                self.progress_manager.print_warning(output);
                1
            }
            ValidationCommandResult::Error(error) => {
                self.progress_manager.print_error(error);
                1
            }
        }
    }
}
