// src/installation.rs
use std::time::{Duration, Instant};

use thiserror::Error;

use crate::{
    command::{CommandError, CommandOutput, CommandRunner},
    config::Config,
    package::{EnvironmentConfig, PackageNode},
};

#[derive(Debug, Clone, PartialEq)]
pub enum InstallationStatus {
    NotStarted,
    Checking,
    NotInstalled,
    AlreadyInstalled,
    Installing,
    Complete,
    Failed(String),
    Skipped(String),
}

#[derive(Debug, Clone)]
pub struct PackageInstallation {
    pub package: PackageNode,
    pub status: InstallationStatus,
    pub start_time: Option<Instant>,
    pub duration: Option<Duration>,
    pub environment: String,
    pub env_config: EnvironmentConfig,
}

#[derive(Error, Debug)]
pub enum InstallationError {
    #[error("Package not compatible with environment: {0}")]
    EnvironmentIncompatible(String),

    #[error("Command execution error: {0}")]
    CommandError(#[from] CommandError),

    #[error("Installation failed: {0}")]
    InstallationFailed(String),

    #[error("Check command failed: {0}")]
    CheckFailed(String),

    #[error("Installation interrupted")]
    Interrupted,
}

impl PackageInstallation {
    pub fn new(package: PackageNode, environment: &str, env_config: EnvironmentConfig) -> Self {
        Self {
            package,
            status: InstallationStatus::NotStarted,
            start_time: None,
            duration: None,
            environment: environment.to_string(),
            env_config,
        }
    }

    pub fn update_status(&mut self, status: InstallationStatus) {
        self.status = status;
    }

    pub fn start(&mut self) {
        self.start_time = Some(Instant::now());
        self.update_status(InstallationStatus::Checking);
    }

    pub fn complete(&mut self, status: InstallationStatus) {
        if let Some(start_time) = self.start_time {
            self.duration = Some(start_time.elapsed());
        }
        self.update_status(status);
    }

    // New helper method to execute commands and handle status updates
    fn execute_command<R: CommandRunner>(
        &mut self,
        runner: &R,
        command: &str,
        initial_status: InstallationStatus,
        error_constructor: impl FnOnce(String) -> InstallationError,
    ) -> Result<CommandOutput, InstallationError> {
        self.update_status(initial_status);

        match runner.execute(command) {
            Ok(output) => Ok(output),
            Err(e) => {
                let error_msg = e.to_string();
                self.update_status(InstallationStatus::Failed(error_msg.clone()));
                Err(error_constructor(error_msg))
            }
        }
    }

    pub fn execute_check<R: CommandRunner>(
        &mut self,
        runner: &R,
    ) -> Result<bool, InstallationError> {
        self.update_status(InstallationStatus::Checking);

        // Clone the check command if it exists to avoid borrowing self.env_config
        let check_cmd = match &self.env_config.check {
            Some(cmd) => cmd.clone(), // Clone the string to end the borrow
            None => {
                self.update_status(InstallationStatus::NotInstalled);
                return Ok(false);
            }
        };

        // Now execute_command can mutably borrow self
        let output =
            self.execute_command(runner, &check_cmd, InstallationStatus::Checking, |e| {
                InstallationError::CheckFailed(e)
            })?;

        let installed = output.success;
        if installed {
            self.update_status(InstallationStatus::AlreadyInstalled);
        } else {
            self.update_status(InstallationStatus::NotInstalled);
        }

        Ok(installed)
    }

    pub fn execute_install<R: CommandRunner>(
        &mut self,
        runner: &R,
    ) -> Result<CommandOutput, InstallationError> {
        // Clone the install command to avoid borrowing self.env_config
        let install_cmd = self.env_config.install.clone();

        // Now execute_command can mutably borrow self
        let output =
            self.execute_command(runner, &install_cmd, InstallationStatus::Installing, |e| {
                InstallationError::CommandError(CommandError::ExecutionError(e))
            })?;

        if output.success {
            self.update_status(InstallationStatus::Complete);
        } else {
            let error_msg = format!("Install command failed with status {}", output.status);
            self.update_status(InstallationStatus::Failed(error_msg.clone()));
            return Err(InstallationError::InstallationFailed(error_msg));
        }

        Ok(output)
    }
}

pub struct InstallationManager<R: CommandRunner> {
    pub runner: R,
    pub config: Config,
}

impl<R: CommandRunner> InstallationManager<R> {
    pub fn new(runner: R, config: Config) -> Self {
        Self { runner, config }
    }

    pub fn install_package(
        &self,
        package: PackageNode,
    ) -> Result<PackageInstallation, InstallationError> {
        // Resolve environment configuration
        let env_config = self
            .config
            .resolve_environment(&package)
            .map_err(|e| InstallationError::EnvironmentIncompatible(e.to_string()))?;

        // Create installation instance
        let mut installation = PackageInstallation::new(
            package.clone(),
            &self.config.environment,
            env_config.clone(),
        );

        // Start the installation process
        installation.start();

        // Check if already installed
        let already_installed = installation.execute_check(&self.runner)?;
        if already_installed {
            installation.complete(InstallationStatus::AlreadyInstalled);
            return Ok(installation);
        }

        // Execute installation
        installation.execute_install(&self.runner)?;
        installation.complete(InstallationStatus::Complete);

        Ok(installation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        command::mock::MockCommandRunner, config::ConfigBuilder, package::PackageNodeBuilder,
    };

    fn create_test_package() -> PackageNode {
        PackageNodeBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment_with_check("test-env", "test install", "test check")
            .build()
    }

    fn create_test_config() -> Config {
        ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .build()
    }

    #[test]
    fn test_installation_status_updates() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();

        let mut installation = PackageInstallation::new(package, "test-env", env_config);
        assert_eq!(installation.status, InstallationStatus::NotStarted);

        installation.update_status(InstallationStatus::Checking);
        assert_eq!(installation.status, InstallationStatus::Checking);

        installation.update_status(InstallationStatus::Installing);
        assert_eq!(installation.status, InstallationStatus::Installing);

        installation.update_status(InstallationStatus::Complete);
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_installation_timing() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();

        let mut installation = PackageInstallation::new(package, "test-env", env_config);
        assert!(installation.start_time.is_none());
        assert!(installation.duration.is_none());

        installation.start();
        assert!(installation.start_time.is_some());
        assert!(installation.duration.is_none());

        installation.complete(InstallationStatus::Complete);
        assert!(installation.duration.is_some());
    }

    #[test]
    fn test_execute_check_no_check_command() {
        let package = PackageNodeBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();

        let result = installation.execute_check(&runner);
        assert!(result.is_ok());
        assert!(!result.unwrap());
        assert_eq!(installation.status, InstallationStatus::NotInstalled);
    }

    #[test]
    fn test_execute_check_installed() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();
        runner.success_response("test check", "Package found");

        let result = installation.execute_check(&runner);
        assert!(result.is_ok());
        assert!(result.unwrap());
        assert_eq!(installation.status, InstallationStatus::AlreadyInstalled);
    }

    #[test]
    fn test_execute_check_not_installed() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();
        runner.error_response("test check", "Not found", 1);

        let result = installation.execute_check(&runner);
        assert!(result.is_ok());
        assert!(!result.unwrap());
        assert_eq!(installation.status, InstallationStatus::NotInstalled);
    }

    #[test]
    fn test_execute_check_error() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();
        runner.add_response(
            "test check",
            Err(CommandError::ExecutionError("Command failed".to_string())),
        );

        let result = installation.execute_check(&runner);
        assert!(result.is_err());
        assert_eq!(
            installation.status,
            InstallationStatus::Failed("Command execution failed: Command failed".to_string())
        );
    }

    #[test]
    fn test_execute_install_success() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();
        runner.success_response("test install", "Installed successfully");

        let result = installation.execute_install(&runner);
        assert!(result.is_ok());
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_execute_install_failure() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();
        runner.error_response("test install", "Installation failed", 1);

        let result = installation.execute_install(&runner);
        assert!(result.is_err());
        assert!(matches!(installation.status, InstallationStatus::Failed(_)));
    }

    #[test]
    fn test_execute_install_error() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = PackageInstallation::new(package, "test-env", env_config);

        let runner = MockCommandRunner::new();
        runner.add_response(
            "test install",
            Err(CommandError::ExecutionError("Command failed".to_string())),
        );

        let result = installation.execute_install(&runner);
        assert!(result.is_err());
        assert!(matches!(installation.status, InstallationStatus::Failed(_)));
    }

    #[test]
    fn test_installation_manager_install_success() {
        let package = create_test_package();
        let config = create_test_config();

        let runner = MockCommandRunner::new();
        runner.error_response("test check", "Not found", 1); // Not installed
        runner.success_response("test install", "Installed successfully");

        let manager = InstallationManager::new(runner, config);
        let result = manager.install_package(package);

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_installation_manager_already_installed() {
        let package = create_test_package();
        let config = create_test_config();

        let runner = MockCommandRunner::new();
        runner.success_response("test check", "Found"); // Already installed

        let manager = InstallationManager::new(runner, config);
        let result = manager.install_package(package);

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::AlreadyInstalled);
    }

    #[test]
    fn test_installation_manager_install_failure() {
        let package = create_test_package();
        let config = create_test_config();

        let runner = MockCommandRunner::new();
        runner.error_response("test check", "Not found", 1); // Not installed
        runner.error_response("test install", "Installation failed", 1);

        let manager = InstallationManager::new(runner, config);
        let result = manager.install_package(package);

        assert!(result.is_err());
    }

    #[test]
    fn test_installation_manager_environment_incompatible() {
        let package = create_test_package();
        let config = ConfigBuilder::default()
            .environment("different-env")
            .package_directory("/test/path")
            .build();

        let runner = MockCommandRunner::new();
        let manager = InstallationManager::new(runner, config);
        let result = manager.install_package(package);

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(InstallationError::EnvironmentIncompatible(_))
        ));
    }
}
