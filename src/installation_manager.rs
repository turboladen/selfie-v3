// src/installation.rs

use crate::{
    domain::config::Config,
    domain::{
        installation::{Installation, InstallationError, InstallationStatus},
        package::Package,
    },
    ports::command::CommandRunner,
};

pub struct InstallationManager<'a, R: CommandRunner> {
    pub runner: &'a R,
    pub config: &'a Config,
}

impl<'a, R: CommandRunner> InstallationManager<'a, R> {
    pub fn new(runner: &'a R, config: &'a Config) -> Self {
        Self { runner, config }
    }

    pub fn install_package(&self, package: Package) -> Result<Installation, InstallationError> {
        // Resolve environment configuration
        let env_config = self
            .config
            .resolve_environment(&package)
            .map_err(|e| InstallationError::EnvironmentIncompatible(e.to_string()))?;

        // Create installation instance
        let mut installation = Installation::new(
            package.clone(),
            &self.config.environment,
            env_config.clone(),
        );

        // Start the installation process
        installation.start();

        // Check if already installed
        let already_installed = installation.execute_check(self.runner)?;
        if already_installed {
            installation.complete(InstallationStatus::AlreadyInstalled);
            return Ok(installation);
        }

        // Execute installation
        installation.execute_install(self.runner)?;
        installation.complete(InstallationStatus::Complete);

        Ok(installation)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use crate::{
        domain::config::ConfigBuilder,
        domain::{installation::Installation, package::PackageBuilder},
        ports::command::{CommandError, MockCommandRunner, MockCommandRunnerExt},
    };

    fn create_test_package() -> Package {
        PackageBuilder::default()
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

    #[test]
    fn test_installation_manager_install_success() {
        let package = create_test_package();
        let config = create_test_config();

        let mut runner = MockCommandRunner::new();
        runner.error_response("test check", "Not found", 1); // Not installed
        runner.success_response("test install", "Installed successfully");

        let manager = InstallationManager::new(&runner, &config);
        let result = manager.install_package(package);

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_installation_manager_already_installed() {
        let package = create_test_package();
        let config = create_test_config();

        let mut runner = MockCommandRunner::new();
        runner.success_response("test check", "Found"); // Already installed

        let manager = InstallationManager::new(&runner, &config);
        let result = manager.install_package(package);

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::AlreadyInstalled);
    }

    #[test]
    fn test_installation_manager_install_failure() {
        let package = create_test_package();
        let config = create_test_config();

        let mut runner = MockCommandRunner::new();
        runner.error_response("test check", "Not found", 1); // Not installed
        runner.error_response("test install", "Installation failed", 1);

        let manager = InstallationManager::new(&runner, &config);
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
        let manager = InstallationManager::new(&runner, &config);
        let result = manager.install_package(package);

        assert!(result.is_err());
        assert!(matches!(
            result,
            Err(InstallationError::EnvironmentIncompatible(_))
        ));
    }
}
