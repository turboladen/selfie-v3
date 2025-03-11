// src/domain/installation.rs
// Installation domain model

use std::time::{Duration, Instant};
use thiserror::Error;

use crate::ports::command::{CommandError, CommandOutput, CommandRunner};

use super::package::{EnvironmentConfig, Package};

/// Represents the current status of a package installation
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InstallationStatus {
    /// Installation has not yet started
    NotStarted,

    /// Currently checking if the package is already installed
    Checking,

    /// Package is not installed
    NotInstalled,

    /// Package is already installed
    AlreadyInstalled,

    /// Package is currently being installed
    Installing,

    /// Installation completed successfully
    Complete,

    /// Installation failed with an error message
    Failed(String),

    /// Installation was skipped for the given reason
    Skipped(String),
}

/// Represents a package installation
#[derive(Debug, Clone)]
pub(crate) struct Installation {
    /// The package being installed
    pub(crate) package: Package,

    /// Current installation status
    pub(crate) status: InstallationStatus,

    /// When the installation started
    pub(crate) start_time: Option<Instant>,

    /// How long the installation took
    pub(crate) duration: Option<Duration>,

    /// The environment name for this installation
    pub(crate) environment: String,

    /// The environment configuration being used
    pub(crate) env_config: EnvironmentConfig,
}

/// Errors that can occur during installation
#[derive(Error, Debug)]
pub(crate) enum InstallationError {
    #[error("Command execution error: {0}")]
    CommandError(CommandError),

    #[error("Installation failed: {0}")]
    InstallationFailed(String),

    #[error("Check command failed: {0}")]
    CheckFailed(String),
}

/// Represents the result of an installation operation
#[derive(Debug)]
pub(crate) struct InstallationResult {
    /// Name of the installed package
    pub(crate) package_name: String,

    /// Final installation status
    pub(crate) status: InstallationStatus,

    /// How long the installation took
    pub(crate) duration: Duration,

    /// Results of dependent package installations
    pub(crate) dependencies: Vec<InstallationResult>,
}

impl Installation {
    /// Create a new installation
    pub(crate) fn new(package: Package, environment: &str, env_config: EnvironmentConfig) -> Self {
        Self {
            package,
            status: InstallationStatus::NotStarted,
            start_time: None,
            duration: None,
            environment: environment.to_string(),
            env_config,
        }
    }

    /// Update the installation status
    pub(crate) fn update_status(&mut self, status: InstallationStatus) {
        self.status = status;
    }

    /// Start the installation
    pub(crate) fn start(&mut self) {
        self.start_time = Some(Instant::now());
        self.update_status(InstallationStatus::Checking);
    }

    /// Complete the installation with the given status
    pub(crate) fn complete(&mut self, status: InstallationStatus) {
        if let Some(start_time) = self.start_time {
            self.duration = Some(start_time.elapsed());
        }
        self.update_status(status);
    }

    // New helper method to execute commands and handle status updates
    fn execute_command(
        &mut self,
        runner: &dyn CommandRunner,
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

    pub(crate) fn execute_check(
        &mut self,
        runner: &dyn CommandRunner,
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

    pub(crate) fn execute_install(
        &mut self,
        runner: &dyn CommandRunner,
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

impl InstallationResult {
    /// Create a new successful installation result
    pub(crate) fn success(package_name: &str, duration: Duration) -> Self {
        Self {
            package_name: package_name.to_string(),
            status: InstallationStatus::Complete,
            duration,
            dependencies: Vec::new(),
        }
    }

    /// Create a result for an already installed package
    pub(crate) fn already_installed(package_name: &str, duration: Duration) -> Self {
        Self {
            package_name: package_name.to_string(),
            status: InstallationStatus::AlreadyInstalled,
            duration,
            dependencies: Vec::new(),
        }
    }

    /// Create a result for a failed installation
    pub(crate) fn failed(
        package_name: &str,
        status: InstallationStatus,
        duration: Duration,
    ) -> Self {
        Self {
            package_name: package_name.to_string(),
            status,
            duration,
            dependencies: Vec::new(),
        }
    }

    /// Add dependencies to the installation result
    pub(crate) fn with_dependencies(mut self, dependencies: Vec<InstallationResult>) -> Self {
        self.dependencies = dependencies;
        self
    }

    /// Calculate the total duration including dependencies
    pub(crate) fn total_duration(&self) -> Duration {
        let mut total = self.duration;
        for dep in &self.dependencies {
            total += dep.duration;
        }
        total
    }

    /// Calculate the duration of dependency installations
    pub(crate) fn dependency_duration(&self) -> Duration {
        let mut total = Duration::from_secs(0);
        for dep in &self.dependencies {
            total += dep.total_duration();
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        domain::package::PackageBuilder,
        ports::command::{MockCommandRunner, MockCommandRunnerExt},
    };

    fn create_test_package() -> Package {
        PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment_with_check("test-env", "test install", "test check")
            .build()
    }

    #[test]
    fn test_installation_status_updates() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();

        let mut installation = Installation::new(package, "test-env", env_config);
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

        let mut installation = Installation::new(package, "test-env", env_config);
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
        let package = PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment("test-env", "test install")
            .build();

        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = Installation::new(package, "test-env", env_config);

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
        let mut installation = Installation::new(package, "test-env", env_config);

        let mut runner = MockCommandRunner::new();
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
        let mut installation = Installation::new(package, "test-env", env_config);

        let mut runner = MockCommandRunner::new();
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
        let mut installation = Installation::new(package, "test-env", env_config);

        let mut runner = MockCommandRunner::new();
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
        let mut installation = Installation::new(package, "test-env", env_config);

        let mut runner = MockCommandRunner::new();
        runner.success_response("test install", "Installed successfully");

        let result = installation.execute_install(&runner);
        assert!(result.is_ok());
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_execute_install_failure() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = Installation::new(package, "test-env", env_config);

        let mut runner = MockCommandRunner::new();
        runner.error_response("test install", "Installation failed", 1);

        let result = installation.execute_install(&runner);
        assert!(result.is_err());
        assert!(matches!(installation.status, InstallationStatus::Failed(_)));
    }

    #[test]
    fn test_execute_install_error() {
        let package = create_test_package();
        let env_config = package.environments.get("test-env").unwrap().clone();
        let mut installation = Installation::new(package, "test-env", env_config);

        let mut runner = MockCommandRunner::new();
        runner.add_response(
            "test install",
            Err(CommandError::ExecutionError("Command failed".to_string())),
        );

        let result = installation.execute_install(&runner);
        assert!(result.is_err());
        assert!(matches!(installation.status, InstallationStatus::Failed(_)));
    }
}
