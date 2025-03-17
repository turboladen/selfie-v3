// src/services/package/installer.rs
mod dependency;

use std::{path::Path, time::Instant};

use console::style;
use dependency::{DependencyResolver, DependencyResolverError};
use thiserror::Error;

use crate::{
    adapters::progress::ProgressManager,
    domain::{
        config::AppConfig,
        errors::{
            EnhancedCommandError, EnhancedDependencyError, EnhancedPackageError, ErrorContext,
        },
        installation::{Installation, InstallationError, InstallationReport, InstallationStatus},
        package::Package,
    },
    ports::{
        command::{CommandError, CommandRunner},
        filesystem::FileSystemError,
        package_repo::{PackageRepoError, PackageRepository},
    },
    services::{command_validator::CommandValidator, enhanced_error_handler::EnhancedErrorHandler},
};

#[derive(Error, Debug)]
pub(crate) enum PackageInstallerError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("FileSystem error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("Package repository error: {0}")]
    PackageRepoError(#[from] PackageRepoError),

    #[error("Dependency error: {0}")]
    DependencyResolverError(#[from] DependencyResolverError),

    #[error("Circular dependency detected: {0}")]
    CircularDependency(String),

    #[error("Installation error: {0}")]
    InstallationError(#[from] InstallationError),

    #[error("Installation canceled by user")]
    InstallationCanceled,

    #[error("Multiple packages found with name: {0}")]
    MultiplePackagesFound(String),

    #[error("Command execution error: {0}")]
    CommandError(#[from] CommandError),

    #[error("Environment error: {0}")]
    EnvironmentError(String),

    #[error("Required command not available: {0}")]
    CommandNotAvailable(String),

    #[error("{0}")]
    EnhancedError(String),
}

// Add conversions from enhanced errors
impl From<EnhancedPackageError> for PackageInstallerError {
    fn from(error: EnhancedPackageError) -> Self {
        match error {
            EnhancedPackageError::PackageNotFound { name, .. } => Self::PackageNotFound(name),
            EnhancedPackageError::MultiplePackagesFound { name, .. } => {
                Self::MultiplePackagesFound(name)
            }
            _ => Self::EnhancedError(error.to_string()),
        }
    }
}

impl From<EnhancedCommandError> for PackageInstallerError {
    fn from(error: EnhancedCommandError) -> Self {
        match error {
            EnhancedCommandError::CommandNotFound { command, .. } => {
                Self::CommandNotAvailable(command)
            }
            _ => Self::CommandError(CommandError::ExecutionError(error.to_string())),
        }
    }
}

impl From<EnhancedDependencyError> for PackageInstallerError {
    fn from(error: EnhancedDependencyError) -> Self {
        match error {
            EnhancedDependencyError::CircularDependency { cycle, .. } => {
                Self::CircularDependency(cycle)
            }
            _ => Self::DependencyResolverError(DependencyResolverError::GraphError(
                crate::domain::dependency::DependencyGraphError::InvalidDependency(
                    error.to_string(),
                ),
            )),
        }
    }
}

pub(crate) struct PackageInstaller<'a> {
    package_repo: &'a dyn PackageRepository,
    error_handler: &'a EnhancedErrorHandler<'a>,
    runner: &'a dyn CommandRunner,
    config: &'a AppConfig,
    progress_manager: &'a ProgressManager,
    check_commands: bool,
    command_validator: CommandValidator<'a>, // Add CommandValidator
}

impl<'a> PackageInstaller<'a> {
    pub(crate) fn new(
        package_repo: &'a dyn PackageRepository,
        error_handler: &'a EnhancedErrorHandler<'_>,
        runner: &'a dyn CommandRunner,
        config: &'a AppConfig,
        progress_manager: &'a ProgressManager,
        check_commands: bool,
    ) -> Self {
        // Create CommandValidator instance
        let command_validator = CommandValidator::new(runner);

        Self {
            package_repo,
            error_handler,
            runner,
            config,
            progress_manager,
            check_commands,
            command_validator,
        }
    }

    /// Install a package by name with enhanced progress reporting and dependency handling
    pub(crate) async fn install_package(
        &self,
        package_name: &str,
    ) -> Result<InstallationReport, PackageInstallerError> {
        // Start timing the entire process
        let start_time = Instant::now();

        let main_package = self.get_package(package_name)?;

        // Print initial package info header
        let header = if self.progress_manager.use_colors() {
            format!(
                "Installing {} (v{}) from {}",
                style(&main_package.name).magenta().bold(),
                main_package.version,
                main_package.path.display()
            )
        } else {
            format!(
                "Installing {} (v{}) from {}",
                main_package.name,
                main_package.version,
                main_package.path.display()
            )
        };

        self.progress_manager.print_info(header);

        // ╭──────────────────────╮
        // │ Resolve dependencies │
        // ╰──────────────────────╯
        let packages = match self.resolve_dependencies(package_name, self.package_repo) {
            Ok(packages) => packages,
            Err(err) => {
                // Use enhanced error handling for dependency errors
                if let DependencyResolverError::CircularDependency(cycle_str) = &err {
                    if let Some(cycle) = self.parse_cycle_string(cycle_str) {
                        let error_msg = self.error_handler.handle_circular_dependency(&cycle);
                        self.progress_manager.print_error(&error_msg);
                    }
                }

                self.progress_manager
                    .print_error(format!("Dependency resolution failed: {}", err));
                return Err(err.into());
            }
        };

        // Pre-flight check: check if all required commands are available
        if self.check_commands && !self.verify_commands(&packages).await? {
            return Err(PackageInstallerError::CommandNotAvailable(
                "Required commands not available".to_string(),
            ));
        }

        // Install all packages in order
        let mut dependency_results = Vec::new();

        // Show dependency section if we have dependencies
        if packages.len() > 1 {
            self.progress_manager.print_info("  Dependencies:");

            // All packages except the last one are dependencies
            for package in packages.iter().take(packages.len() - 1) {
                self.install_dependency(package, start_time, &mut dependency_results)
                    .await?
            }
        }

        // Now install the main package
        let main_package = packages.last().unwrap();
        let main_result = self.install_single_package(main_package, 2).await?;

        // Get the total installation time and create the final result
        let total_duration = start_time.elapsed();
        let mut final_result = main_result.with_dependencies(dependency_results);

        // Override the duration with the main package duration only
        final_result.duration = total_duration;

        // Print summary
        self.progress_manager.print_progress("\n");
        self.report_final_status(&final_result);

        Ok(final_result)
    }

    async fn install_dependency(
        &self,
        package: &Package,
        start_time: Instant,
        dependency_results: &mut Vec<InstallationReport>,
    ) -> Result<(), PackageInstallerError> {
        // Show dependency name and version
        let dependency_header = if self.progress_manager.use_colors() {
            format!(
                "    Installing {} (v{}) from {}",
                style(&package.name).magenta(),
                package.version,
                package.path.display()
            )
        } else {
            format!(
                "    Installing {} (v{}) from {}",
                package.name,
                package.version,
                package.path.display()
            )
        };
        self.progress_manager.print_info(dependency_header);

        // Make sure the dep has info for this environment
        if !package.environments.contains_key(self.config.environment()) {
            dependency_results.push(InstallationReport {
                package_name: package.name.clone(),
                status: InstallationStatus::Skipped(format!(
                    "Package `{}` does not support current environment (`{}` section)",
                    &package.name,
                    self.config.environment()
                )),
                duration: start_time.elapsed(),
                dependencies: vec![],
                command_output: None,
            });

            return Ok(());
        }

        // Install the dependency
        match self.install_single_package(package, 6).await {
            Ok(result) => {
                // Only continue if installation was successful or package was already installed
                match result.status {
                    InstallationStatus::Complete
                    | InstallationStatus::AlreadyInstalled
                    | InstallationStatus::Skipped(_) => {
                        dependency_results.push(result);
                    }
                    _ => {
                        self.progress_manager
                            .print_error("      ✗ Dependency installation failed");

                        return Err(PackageInstallerError::InstallationError(
                            InstallationError::InstallationFailed(format!(
                                "Dependency '{}' installation failed: {:?}",
                                package.name, result.status
                            )),
                        ));
                    }
                }
            }
            Err(err) => {
                self.progress_manager.print_error(format!(
                    "      ✗ Failed to install dependency '{}': {}",
                    package.name, err,
                ));
                return Err(err);
            }
        }

        Ok(())
    }

    /// Check if a package can be installed in the current environment
    pub(crate) async fn check_package_installable(
        &self,
        package_name: &str,
    ) -> Result<bool, PackageInstallerError> {
        // Find the package
        let package = self.get_package(package_name)?;

        // Check if package supports current environment
        if !package.environments.contains_key(self.config.environment()) {
            return Ok(false);
        }

        // Check if required commands are available
        if self.check_commands {
            if let Some(env_config) = package.environments.get(self.config.environment()) {
                if let Some(base_cmd) = CommandValidator::extract_base_command(&env_config.install)
                {
                    let availability_result = self
                        .command_validator
                        .check_command_availability(self.config.environment(), base_cmd)
                        .await;

                    if !availability_result.is_available {
                        return Ok(false);
                    }
                }
            }
        }

        // Check dependencies if requested
        let _ = self.resolve_dependencies(package_name, self.package_repo)?;

        // If we got here, the package is installable
        Ok(true)
    }

    fn get_package(&self, package_name: &str) -> Result<Package, PackageInstallerError> {
        self.package_repo
            .get_package(package_name)
            .map_err(|e| match e {
                PackageRepoError::DirectoryNotFound(dir_path) => {
                    // This is a path not found error
                    let error_msg = self
                        .error_handler
                        .handle_path_not_found(Path::new(&dir_path));
                    PackageInstallerError::EnhancedError(error_msg)
                }
                PackageRepoError::IoError(ref io_err) => {
                    // Check if it's a file not found error
                    if io_err.kind() == std::io::ErrorKind::NotFound {
                        // Unfortunately we don't have the specific path here
                        // We could extract it from the error message
                        let error_text = io_err.to_string();
                        PackageInstallerError::EnhancedError(error_text)
                    } else {
                        PackageInstallerError::PackageRepoError(e)
                    }
                }
                PackageRepoError::PackageNotFound(name) => {
                    // Use enhanced error handling for not found errors
                    let error_msg = self.error_handler.handle_package_not_found(&name);
                    PackageInstallerError::EnhancedError(error_msg)
                }
                PackageRepoError::MultiplePackagesFound(name) => {
                    PackageInstallerError::MultiplePackagesFound(name)
                }
                _ => PackageInstallerError::PackageRepoError(e),
            })
    }

    /// Resolve dependencies for a package
    fn resolve_dependencies(
        &self,
        package_name: &str,
        package_repo: &dyn PackageRepository,
    ) -> Result<Vec<Package>, DependencyResolverError> {
        // Create dependency resolver and resolve dependencies
        DependencyResolver::new(package_repo, self.config).resolve_dependencies(package_name)
    }

    /// Verify that all required commands are available
    async fn verify_commands(&self, packages: &[Package]) -> Result<bool, PackageInstallerError> {
        // Check commands for each package
        let mut missing_commands = Vec::new();

        for package in packages {
            if let Some(env_config) = package.environments.get(self.config.environment()) {
                // Extract and check base command
                if let Some(base_cmd) = CommandValidator::extract_base_command(&env_config.install)
                {
                    let availability_result = self
                        .command_validator
                        .check_command_availability(self.config.environment(), base_cmd)
                        .await;

                    if !availability_result.is_available {
                        missing_commands.push((package.name.clone(), base_cmd.to_string()));
                    }
                }
            }
        }

        // If any commands are missing, report and return false
        if !missing_commands.is_empty() {
            let mut error_msg = String::from(
                "The following commands required for installation are not available:\n\n",
            );
            for (pkg, cmd) in missing_commands {
                error_msg.push_str(&format!("  • Package '{}' requires '{}'\n", pkg, cmd));
            }
            error_msg.push_str("\nPlease install these commands and try again.");
            return Err(PackageInstallerError::CommandNotAvailable(error_msg));
        }

        Ok(true)
    }

    /// Install a single package (no dependency handling) with progress reporting
    async fn install_single_package(
        &self,
        package: &Package,
        indent_level: usize,
    ) -> Result<InstallationReport, PackageInstallerError> {
        let indent = " ".repeat(indent_level);

        // Resolve environment configuration with enhanced error context
        let env_config = self.config.resolve_environment(package).map_err(|e| {
            // Create an enhanced error with context
            let context = ErrorContext::default()
                .with_package(&package.name)
                .with_environment(self.config.environment())
                .with_message(&format!("Original error: {}", e));

            let enhanced_error = EnhancedPackageError::environment_not_supported(
                self.config.environment(),
                &package.name,
            )
            .with_context(context);

            let user_message = self
                .error_handler
                .handle_environment_not_found(self.config.environment(), &package.name);

            PackageInstallerError::EnhancedError(user_message)
        })?;

        // Create installation and start it
        let installation = Installation::new(env_config.clone()).start();

        // Check if already installed
        let installation = match installation.execute_check(self.runner).await {
            Ok(state) => state,
            Err(err) => return Err(PackageInstallerError::InstallationError(err)),
        };

        // Handle the result based on the state
        match &installation {
            Installation::AlreadyInstalled { check_duration, .. } => {
                // Print "Already installed" with duration
                let status_message = format!(
                    "{}✓ Checking installation status: Already installed ({:.1?})",
                    indent, check_duration
                );
                self.progress_manager.print_success(status_message);

                // Return the result directly - no need to install
                return installation
                    .into_result(package.name.clone())
                    .map_err(PackageInstallerError::InstallationError);
            }
            Installation::NotAlreadyInstalled { check_duration, .. } => {
                // Print "Not installed" with duration
                self.progress_manager
                    .print_progress(self.progress_manager.with_duration(
                        format!("{}✓ Checking installation status: Not installed", indent),
                        Some(*check_duration),
                    ));
            }
            Installation::Failed { error_message, .. } => {
                // Check failed, print error and return
                self.progress_manager.print_error(format!(
                    "{}✗ Checking installation status failed: {}",
                    indent, error_message
                ));
                return installation
                    .into_result(package.name.clone())
                    .map_err(PackageInstallerError::InstallationError);
            }
            _ => {
                // Shouldn't get here with proper state transitions
                return Err(PackageInstallerError::InstallationError(
                    InstallationError::InvalidState(format!(
                        "Unexpected state after check: {:?}",
                        installation.status()
                    )),
                ));
            }
        }

        // Print installing message
        self.progress_manager
            .print_progress(format!("{}⌛ Installing...", indent));

        // Execute installation
        let installation = match installation.execute_install(self.runner).await {
            Ok(state) => state,
            Err(err) => return Err(PackageInstallerError::InstallationError(err)),
        };

        // Handle the result based on the final state
        match &installation {
            Installation::Complete { duration, .. } => {
                // Print completion message
                let complete_message =
                    format!("{}✓ Installation complete ({:.1?})", indent, duration);
                self.progress_manager.print_success(complete_message);
            }
            Installation::Failed { error_message, .. } => {
                // Print error message
                let error_message = format!("{}✗ Installation failed: {}", indent, error_message);
                self.progress_manager.print_error(error_message);
            }
            _ => {
                // Shouldn't get here with proper state transitions
                return Err(PackageInstallerError::InstallationError(
                    InstallationError::InvalidState(format!(
                        "Unexpected state after install: {:?}",
                        installation.status()
                    )),
                ));
            }
        }

        // Print verbose output if enabled
        if self.progress_manager.verbose() {
            if let Installation::Complete { command_output, .. } = &installation {
                self.progress_manager.print_verbose("Command stdout:");
                for line in command_output.stdout.lines() {
                    self.progress_manager.print_verbose(format!("  {}", line));
                }

                if !command_output.stderr.is_empty() {
                    self.progress_manager.print_verbose("Command stderr:");
                    for line in command_output.stderr.lines() {
                        self.progress_manager.print_verbose(format!("  {}", line));
                    }
                }
            }
        }

        // Return the final result
        installation
            .into_result(package.name.clone())
            .map_err(PackageInstallerError::InstallationError)
    }

    /// Report the final installation status with timing information
    fn report_final_status(&self, result: &InstallationReport) {
        let total_duration = result.total_duration();
        let dep_duration = result.dependency_duration();
        let package_duration = result.duration;

        self.progress_manager.print_success(format!(
            "\nPackage '{}' installation summary:",
            result.package_name
        ));
        // Display timing information according to spec
        if !result.dependencies.is_empty() {
            self.progress_manager
                .print_with_duration("Total time", Some(total_duration));
            self.progress_manager
                .print_with_duration("Dependencies:", Some(dep_duration));
            self.progress_manager
                .print_with_duration("Package:", Some(package_duration));
        } else {
            self.progress_manager
                .print_with_duration("Total time:", Some(total_duration));
        }
    }

    /// Extract the base command from a command string
    fn extract_base_command(command: &str) -> Option<&str> {
        CommandValidator::extract_base_command(command)
    }

    /// Parse a cycle string into a vector of package names
    fn parse_cycle_string(&self, cycle_str: &str) -> Option<Vec<String>> {
        // Example format: "package1 -> package2 -> package3"
        let output = cycle_str
            .split(" -> ")
            .map(|s| s.trim().to_string())
            .collect::<Vec<String>>()
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>();

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{config::AppConfigBuilder, package::PackageBuilder},
        ports::{
            command::MockCommandRunner, filesystem::MockFileSystem,
            package_repo::MockPackageRepository,
        },
    };

    fn create_test_package() -> Package {
        PackageBuilder::default()
            .name("test-package")
            .version("1.0.0")
            .environment_with_check("test-env", "test install", "test check")
            .build()
    }

    fn create_test_config() -> AppConfig {
        AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .build()
    }

    fn create_installer_deps() -> (
        MockFileSystem,
        MockCommandRunner,
        MockPackageRepository,
        ProgressManager,
    ) {
        (
            MockFileSystem::new(),
            MockCommandRunner::new(),
            MockPackageRepository::default(),
            ProgressManager::new(false, true),
        )
    }

    #[tokio::test]
    async fn test_install_success() {
        let package = create_test_package();
        let config = create_test_config();
        let (fs, mut runner, mut repo, progress_manager) = create_installer_deps();

        repo.mock_get_package_ok(&package.name, package.clone());

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);

        runner.mock_execute_success_1("test check", "Not found");
        runner.mock_execute_success_0("test install", "Installed successfully");
        runner.mock_is_command_available("test", true);

        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, true);
        let result = installer.install_package(&package.name).await;

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[tokio::test]
    async fn test_already_installed() {
        let package = create_test_package();
        let config = create_test_config();
        let (fs, mut runner, mut repo, progress_manager) = create_installer_deps();

        repo.mock_get_package_ok(&package.name, package.clone());

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);

        runner.mock_execute_success_0("test check", "Found"); // Already installed
        runner.mock_is_command_available("test", true);

        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, true);
        let result = installer.install_package(&package.name).await;

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::AlreadyInstalled);
    }

    #[tokio::test]
    async fn test_install_failure() {
        let package = create_test_package();
        let config = create_test_config();
        let (fs, mut runner, mut repo, progress_manager) = create_installer_deps();

        repo.mock_get_package_ok(&package.name, package.clone());

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);

        runner.mock_execute_success_1("test check", "Not found");
        runner.mock_execute_success_1("test install", "Installation failed");
        runner.mock_is_command_available("test", true);

        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, true);
        let result = installer.install_package(&package.name).await;
        dbg!(&result);

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_environment_incompatible() {
        let package = create_test_package();
        let config = AppConfigBuilder::default()
            .environment("different-env")
            .package_directory("/test/path")
            .build();
        let (fs, runner, mut repo, progress_manager) = create_installer_deps();

        repo.mock_get_package_ok(&package.name, package.clone());

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);

        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, true);
        let result = installer.install_package(&package.name).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cycle_string() {
        let config = create_test_config();
        let (fs, runner, repo, progress_manager) = create_installer_deps();
        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);
        let manager = PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, true);

        // Test basic cycle parsing
        let cycle_str = "package1 -> package2 -> package3 -> package1";
        let parsed = manager.parse_cycle_string(cycle_str).unwrap();
        assert_eq!(
            parsed,
            vec![
                "package1".to_string(),
                "package2".to_string(),
                "package3".to_string(),
                "package1".to_string(),
            ]
        );

        // Test empty string
        let empty = manager.parse_cycle_string("");
        assert!(empty.is_none() || empty.unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_package_install_end_to_end() {
        // Create mock environment
        let (fs, mut runner, mut repo, progress_manager) = create_installer_deps();

        // Create config
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        // Set up package files in the filesystem
        let package_yaml = r#"
        name: ripgrep
        version: 1.0.0
        environments:
          test-env:
            install: rg install
            check: rg check
    "#;

        repo.mock_get_package_ok("ripgrep", Package::from_yaml(package_yaml).unwrap());

        // Set up mock command responses
        runner.mock_execute_success_1("rg check", "Not found");
        runner.mock_execute_success_0("rg install", "Installed successfully");

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);

        let progress_manager = ProgressManager::new(false, true);

        // Create package installer (using the new consolidated version)
        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, false);

        // Run the installation
        let result = installer.install_package("ripgrep").await;

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
    }

    #[tokio::test]
    async fn test_package_install_with_dependencies() {
        // Create mock environment
        let (fs, mut runner, mut repo, progress_manager) = create_installer_deps();

        // Create config
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        // Set up package files in the filesystem
        let package_yaml = r#"
        name: ripgrep
        version: 1.0.0
        environments:
          test-env:
            install: rg install
            check: rg check
            dependencies:
              - rust
    "#;

        let dependency_yaml = r#"
        name: rust
        version: 1.0.0
        environments:
          test-env:
            install: rust install
            check: rust check
    "#;

        repo.mock_get_package_ok("ripgrep", Package::from_yaml(package_yaml).unwrap());
        repo.mock_get_package_ok("rust", Package::from_yaml(dependency_yaml).unwrap());

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);

        // Set up mock command responses
        runner.mock_execute_success_1("rg check", "Not found");
        runner.mock_execute_success_0("rg install", "Installed successfully");
        runner.mock_execute_success_1("rust check", "Not found");
        runner.mock_execute_success_0("rust install", "Installed successfully");

        let progress_manager = ProgressManager::new(false, true);

        // Create package installer
        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, false);

        // Run the installation
        let result = installer.install_package("ripgrep").await;

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.dependencies.len(), 1);
        assert_eq!(install_result.dependencies[0].package_name, "rust");
    }

    // Update the test in tests/integration_test.rs to test dependency resolution

    #[tokio::test]
    async fn test_package_install_with_complex_dependencies() {
        // Create mock environment
        let (fs, mut runner, mut repo, progress_manager) = create_installer_deps();

        // Create config
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        // Set up package files with a dependency chain
        let main_pkg_yaml = r#"
        name: main-pkg
        version: 1.0.0
        environments:
          test-env:
            install: main-install
            check: main-check
            dependencies:
              - dep1
              - dep2
    "#;

        let dep1_yaml = r#"
        name: dep1
        version: 1.0.0
        environments:
          test-env:
            install: dep1-install
            check: dep1-check
            dependencies:
              - dep3
    "#;

        let dep2_yaml = r#"
        name: dep2
        version: 1.0.0
        environments:
          test-env:
            install: dep2-install
            check: dep2-check
    "#;

        let dep3_yaml = r#"
        name: dep3
        version: 1.0.0
        environments:
          test-env:
            install: dep3-install
            check: dep3-check
    "#;
        repo.mock_get_package_ok("main-pkg", Package::from_yaml(main_pkg_yaml).unwrap());
        repo.mock_get_package_ok("dep1", Package::from_yaml(dep1_yaml).unwrap());
        repo.mock_get_package_ok("dep2", Package::from_yaml(dep2_yaml).unwrap());
        repo.mock_get_package_ok("dep3", Package::from_yaml(dep3_yaml).unwrap());

        // Set up mock command responses - all need to be installed
        runner.mock_execute_success_1("main-check", "Not found");
        runner.mock_execute_success_0("main-install", "Installed successfully");
        runner.mock_execute_success_1("dep1-check", "Not found");
        runner.mock_execute_success_0("dep1-install", "Installed successfully");
        runner.mock_execute_success_1("dep2-check", "Not found");
        runner.mock_execute_success_0("dep2-install", "Installed successfully");
        runner.mock_execute_success_1("dep3-check", "Not found");
        runner.mock_execute_success_0("dep3-install", "Installed successfully");

        let eeh = EnhancedErrorHandler::new(&fs, &repo, &progress_manager);
        let progress_manager = ProgressManager::new(false, true);

        // Create package installer
        let installer =
            PackageInstaller::new(&repo, &eeh, &runner, &config, &progress_manager, false);

        // Run the installation
        let result = installer.install_package("main-pkg").await;

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();

        // Correct dependencies were installed
        assert_eq!(install_result.package_name, "main-pkg");
        assert_eq!(install_result.status, InstallationStatus::Complete);

        // All dependencies were installed (3 of them)
        assert_eq!(install_result.dependencies.len(), 3);

        // dep3 should be first (deepest dependency)
        let dep3_result = install_result
            .dependencies
            .iter()
            .find(|d| d.package_name == "dep3")
            .unwrap();
        assert_eq!(dep3_result.status, InstallationStatus::Complete);

        // dep1 and dep2 should both be present
        let dep1_result = install_result
            .dependencies
            .iter()
            .find(|d| d.package_name == "dep1")
            .unwrap();
        assert_eq!(dep1_result.status, InstallationStatus::Complete);

        let dep2_result = install_result
            .dependencies
            .iter()
            .find(|d| d.package_name == "dep2")
            .unwrap();
        assert_eq!(dep2_result.status, InstallationStatus::Complete);
    }
}
