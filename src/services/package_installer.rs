// src/services/package_installer.rs
mod dependency;

use std::time::{Duration, Instant};

use console::style;
use dependency::{DependencyResolver, DependencyResolverError};
use thiserror::Error;

use crate::{
    adapters::{
        package_repo::yaml::YamlPackageRepository,
        progress::{ProgressManager, ProgressStyleType},
    },
    domain::{
        config::AppConfig,
        installation::{Installation, InstallationError, InstallationStatus},
        package::Package,
    },
    ports::{
        command::{CommandError, CommandOutput, CommandRunner},
        filesystem::{FileSystem, FileSystemError},
        package_repo::{PackageRepoError, PackageRepository},
    },
};

#[derive(Error, Debug)]
pub enum PackageInstallerError {
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
}

#[derive(Debug)]
pub struct InstallationResult {
    pub package_name: String,
    pub status: InstallationStatus,
    pub duration: Duration,
    pub command_output: Option<CommandOutput>,
    pub dependencies: Vec<InstallationResult>,
}

impl InstallationResult {
    pub fn success(package_name: &str, duration: Duration, output: Option<CommandOutput>) -> Self {
        Self {
            package_name: package_name.to_string(),
            status: InstallationStatus::Complete,
            duration,
            command_output: output,
            dependencies: Vec::new(),
        }
    }

    pub fn already_installed(package_name: &str, duration: Duration) -> Self {
        Self {
            package_name: package_name.to_string(),
            status: InstallationStatus::AlreadyInstalled,
            duration,
            command_output: None,
            dependencies: Vec::new(),
        }
    }

    pub fn failed(package_name: &str, status: InstallationStatus, duration: Duration) -> Self {
        Self {
            package_name: package_name.to_string(),
            status,
            duration,
            command_output: None,
            dependencies: Vec::new(),
        }
    }

    pub fn with_dependencies(mut self, dependencies: Vec<InstallationResult>) -> Self {
        self.dependencies = dependencies;
        self
    }

    pub fn total_duration(&self) -> Duration {
        let mut total = self.duration;
        for dep in &self.dependencies {
            total += dep.duration;
        }
        total
    }

    pub fn dependency_duration(&self) -> Duration {
        let mut total = Duration::from_secs(0);
        for dep in &self.dependencies {
            total += dep.total_duration();
        }
        total
    }
}

pub struct PackageInstaller<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    runner: &'a R,
    config: &'a AppConfig,
    progress_manager: &'a ProgressManager,
    check_commands: bool,
}

impl<'a, F: FileSystem, R: CommandRunner> PackageInstaller<'a, F, R> {
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: &'a AppConfig,
        progress_manager: &'a ProgressManager,
        check_commands: bool,
    ) -> Self {
        Self {
            fs,
            runner,
            config,
            progress_manager,
            check_commands,
        }
    }

    /// Install a package by name with enhanced progress reporting and dependency handling
    pub fn install_package(
        &self,
        package_name: &str,
    ) -> Result<InstallationResult, PackageInstallerError> {
        // Create main progress display with proper styling
        let main_message = if self.progress_manager.use_colors() {
            format!(
                "Installing package '{}'",
                style(package_name).magenta().bold()
            )
        } else {
            format!("Installing package '{}'", package_name)
        };

        let main_progress = self.progress_manager.create_progress_bar(
            "main",
            &main_message,
            ProgressStyleType::Message,
        );

        // Create package repository
        let package_repo =
            YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());

        // Update progress
        main_progress.set_message("Resolving dependencies...");

        // Start timing the entire process
        let start_time = Instant::now();

        // Resolve dependencies
        let packages = match self.resolve_dependencies(package_name, &package_repo) {
            Ok(packages) => packages,
            Err(err) => {
                main_progress.abandon_with_message("Dependency resolution failed");
                return Err(err);
            }
        };

        // Pre-flight check: check if all required commands are available
        if self.check_commands && !self.verify_commands(&packages, &main_progress)? {
            return Err(PackageInstallerError::CommandNotAvailable(
                "Required commands not available".to_string(),
            ));
        }

        // Show dependency information
        if packages.len() > 1 {
            let deps_count = packages.len() - 1;
            let deps_message = if self.progress_manager.use_colors() {
                format!(
                    "Found {} packages to install (including {} dependencies)",
                    style(packages.len()).cyan(),
                    style(deps_count).cyan()
                )
            } else {
                format!(
                    "Found {} packages to install (including {} dependencies)",
                    packages.len(),
                    deps_count
                )
            };
            main_progress.set_message(deps_message);
        }

        // Get the main package (last in the list)
        let main_package = packages.last().unwrap().clone();

        // Install all packages in order
        let mut dependency_results = Vec::new();

        // All packages except the last one are dependencies
        for package in packages.iter().take(packages.len() - 1) {
            let dep_id = format!("dep-{}", package.name);

            // Create colored dependency message
            let dep_message = if self.progress_manager.use_colors() {
                format!("Installing dependency '{}'", style(&package.name).magenta())
            } else {
                format!("Installing dependency '{}'", &package.name)
            };

            let dep_progress = self.progress_manager.create_progress_bar(
                &dep_id,
                &dep_message,
                ProgressStyleType::Spinner,
            );

            // Install dependency and stop immediately on failure
            match self.install_single_package(package, &dep_id) {
                Ok(result) => {
                    // Only continue if installation was successful or package was already installed
                    match result.status {
                        InstallationStatus::Complete | InstallationStatus::AlreadyInstalled => {
                            dependency_results.push(result);
                        }
                        _ => {
                            // For any other status (like Failed), return an error to stop the process
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
                    // Log error in progress bar
                    let error_msg =
                        format!("Failed to install dependency '{}': {}", package.name, err);
                    dep_progress.abandon_with_message(error_msg);

                    // Immediately return any error during dependency installation
                    return Err(err);
                }
            }
        }

        // Only install the main package if all dependencies were successfully installed
        let main_id = format!("pkg-{}", main_package.name);

        // Create colored main package message
        let main_message = if self.progress_manager.use_colors() {
            format!(
                "Installing '{}'",
                style(&main_package.name).magenta().bold()
            )
        } else {
            format!("Installing '{}'", &main_package.name)
        };

        let main_pb = self.progress_manager.create_progress_bar(
            &main_id,
            &main_message,
            ProgressStyleType::Spinner,
        );

        // Only show the full "and dependencies" message if there are actual dependencies
        if !dependency_results.is_empty() {
            let deps_message = if self.progress_manager.use_colors() {
                format!(
                    "Installing '{}' and {} dependencies...",
                    style(&main_package.name).magenta().bold(),
                    style(dependency_results.len()).cyan()
                )
            } else {
                format!(
                    "Installing '{}' and {} dependencies...",
                    main_package.name,
                    dependency_results.len()
                )
            };

            main_pb.set_message(deps_message);
        }

        let main_result = self.install_single_package(&main_package, &main_id)?;

        // Get the total installation time
        let total_duration = start_time.elapsed();

        // Create the final result with dependencies
        let mut final_result = main_result.with_dependencies(dependency_results);

        // Override the duration with the total time
        final_result.duration = total_duration;

        // Report final status including timing information
        self.report_final_status(&final_result);

        Ok(final_result)
    }

    /// Check if a package can be installed in the current environment
    pub fn check_package_installable(
        &self,
        package_name: &str,
    ) -> Result<bool, PackageInstallerError> {
        // Create a package repository
        let package_repo =
            YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());

        // Find the package
        let package = package_repo
            .get_package(package_name)
            .map_err(|e| match e {
                PackageRepoError::PackageNotFound(name) => {
                    PackageInstallerError::PackageNotFound(name)
                }
                PackageRepoError::MultiplePackagesFound(name) => {
                    PackageInstallerError::MultiplePackagesFound(name)
                }
                _ => PackageInstallerError::PackageRepoError(e),
            })?;

        // Check if package supports current environment
        if !package.environments.contains_key(self.config.environment()) {
            return Ok(false);
        }

        // Check if required commands are available
        if self.check_commands {
            if let Some(env_config) = package.environments.get(self.config.environment()) {
                if let Some(base_cmd) = Self::extract_base_command(&env_config.install) {
                    if !self.runner.is_command_available(base_cmd) {
                        return Ok(false);
                    }
                }
            }
        }

        // Check dependencies if requested
        let _ = self.resolve_dependencies(package_name, &package_repo)?;

        // If we got here, the package is installable
        Ok(true)
    }

    /// Resolve dependencies for a package
    fn resolve_dependencies(
        &self,
        package_name: &str,
        package_repo: &impl PackageRepository,
    ) -> Result<Vec<Package>, PackageInstallerError> {
        // Create dependency resolver and resolve dependencies
        let resolver = DependencyResolver::new(package_repo, self.config);

        Ok(resolver.resolve_dependencies(package_name)?)
    }

    /// Verify that all required commands are available
    fn verify_commands(
        &self,
        packages: &[Package],
        progress: &indicatif::ProgressBar,
    ) -> Result<bool, PackageInstallerError> {
        // Check commands for each package
        let mut missing_commands = Vec::new();

        for package in packages {
            if let Some(env_config) = package.environments.get(self.config.environment()) {
                // Extract and check base command
                if let Some(base_cmd) = Self::extract_base_command(&env_config.install) {
                    if !self.runner.is_command_available(base_cmd) {
                        missing_commands.push((package.name.clone(), base_cmd.to_string()));
                    }
                }
            }
        }

        // If any commands are missing, report and return false
        if !missing_commands.is_empty() {
            progress.abandon_with_message("Command availability check failed");
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
    fn install_single_package(
        &self,
        package: &Package,
        progress_id: &str,
    ) -> Result<InstallationResult, PackageInstallerError> {
        let start_time = Instant::now();

        // Get or create progress bar for this package
        let progress_bar = match self.progress_manager.get_progress_bar(progress_id) {
            Some(pb) => pb,
            None => {
                let message = if self.progress_manager.use_colors() {
                    format!("Installing '{}'", style(&package.name).magenta().bold())
                } else {
                    format!("Installing '{}'", package.name)
                };

                self.progress_manager.create_progress_bar(
                    progress_id,
                    &message,
                    ProgressStyleType::Spinner,
                )
            }
        };

        // Update the message with colors
        let install_message = if self.progress_manager.use_colors() {
            format!("Installing '{}'", style(&package.name).magenta().bold())
        } else {
            format!("Installing '{}'", package.name)
        };

        progress_bar.set_message(install_message);

        // If check_commands is enabled, double-check command availability
        if self.check_commands {
            if let Some(env_config) = package.environments.get(self.config.environment()) {
                if let Some(base_cmd) = Self::extract_base_command(&env_config.install) {
                    if !self.runner.is_command_available(base_cmd) {
                        let duration = start_time.elapsed();
                        let error_msg = format!(
                            "Command '{}' required for installation is not available",
                            base_cmd
                        );

                        // Update progress bar with error
                        self.progress_manager
                            .update_from_status(
                                progress_id,
                                &InstallationStatus::Failed(error_msg.clone()),
                                Some(duration),
                            )
                            .map_err(PackageInstallerError::EnvironmentError)?;

                        return Err(PackageInstallerError::CommandNotAvailable(error_msg));
                    }
                }
            }
        }

        // Resolve environment configuration
        let env_config = self
            .config
            .resolve_environment(package)
            .map_err(|e| PackageInstallerError::EnvironmentError(e.to_string()))?;

        // Create installation instance
        let mut installation = Installation::new(
            package.clone(),
            self.config.environment(),
            env_config.clone(),
        );

        // Update progress status to checking
        self.progress_manager
            .update_from_status(progress_id, &InstallationStatus::Checking, None)
            .map_err(PackageInstallerError::EnvironmentError)?;

        // Start the installation process
        installation.start();

        // Check if already installed
        let already_installed = installation
            .execute_check(self.runner)
            .map_err(PackageInstallerError::InstallationError)?;

        if already_installed {
            let duration = start_time.elapsed();
            installation.complete(InstallationStatus::AlreadyInstalled);

            // Update progress bar
            self.progress_manager
                .update_from_status(
                    progress_id,
                    &InstallationStatus::AlreadyInstalled,
                    Some(duration),
                )
                .map_err(PackageInstallerError::EnvironmentError)?;

            return Ok(InstallationResult::already_installed(
                &package.name,
                duration,
            ));
        }

        // Execute installation
        let result = match installation.execute_install(self.runner) {
            Ok(output) => {
                let duration = start_time.elapsed();
                installation.complete(InstallationStatus::Complete);

                // Update progress bar
                self.progress_manager
                    .update_from_status(progress_id, &InstallationStatus::Complete, Some(duration))
                    .map_err(PackageInstallerError::EnvironmentError)?;

                InstallationResult::success(&package.name, duration, Some(output))
            }
            Err(err) => {
                let duration = start_time.elapsed();
                installation.update_status(InstallationStatus::Failed(format!(
                    "Installation error: {}",
                    err
                )));

                // Update progress bar with error
                self.progress_manager
                    .update_from_status(
                        progress_id,
                        &InstallationStatus::Failed(err.to_string()),
                        Some(duration),
                    )
                    .map_err(PackageInstallerError::EnvironmentError)?;

                return Err(PackageInstallerError::InstallationError(err));
            }
        };

        Ok(result)
    }

    /// Report the final installation status with timing information
    fn report_final_status(&self, result: &InstallationResult) {
        let total_duration = result.total_duration();
        let dep_duration = result.dependency_duration();
        let package_duration = result.duration;

        // Create a summary progress bar
        let summary_pb = self.progress_manager.create_progress_bar(
            "summary",
            "Summary",
            ProgressStyleType::Message,
        );

        // Display summary information with colors where appropriate
        if !result.dependencies.is_empty() {
            if self.progress_manager.use_colors() {
                summary_pb.println(format!(
                    "Total time: {}",
                    style(format!("{:.1?}", total_duration)).cyan()
                ));
                summary_pb.println(format!(
                    "Dependencies: {}",
                    style(format!("{:.1?}", dep_duration)).cyan()
                ));
                summary_pb.println(format!(
                    "Package: {}",
                    style(format!("{:.1?}", package_duration)).cyan()
                ));
            } else {
                summary_pb.println(format!("Total time: {:.1?}", total_duration));
                summary_pb.println(format!("Dependencies: {:.1?}", dep_duration));
                summary_pb.println(format!("Package: {:.1?}", package_duration));
            }
        } else if self.progress_manager.use_colors() {
            summary_pb.println(format!(
                "Total time: {}",
                style(format!("{:.1?}", total_duration)).cyan()
            ));
        } else {
            summary_pb.println(format!("Total time: {:.1?}", total_duration));
        }

        // Special handling for already installed packages
        if matches!(result.status, InstallationStatus::AlreadyInstalled) {
            // Just show a generic success message, since "Already installed" was already shown
            summary_pb.finish_with_message("Done");
            return;
        }

        // Handle other status cases
        match result.status {
            InstallationStatus::Complete => {
                let success_message = if self.progress_manager.use_colors() {
                    format!(
                        "Successfully installed '{}' and {} dependencies",
                        style(&result.package_name).magenta().bold(),
                        style(result.dependencies.len()).cyan()
                    )
                } else {
                    format!(
                        "Successfully installed '{}' and {} dependencies",
                        result.package_name,
                        result.dependencies.len()
                    )
                };
                summary_pb.finish_with_message("Complete");
                println!("{}", success_message);
            }
            InstallationStatus::AlreadyInstalled => {
                // This case is handled above, but included for completeness
            }
            _ => {
                let error_message = if self.progress_manager.use_colors() {
                    format!(
                        "Failed to install '{}'",
                        style(&result.package_name).magenta().bold()
                    )
                } else {
                    format!("Failed to install '{}'", result.package_name)
                };
                summary_pb.abandon_with_message("Failed");
                eprintln!("{}", error_message);
            }
        }
    }

    /// Extract the base command from a command string
    fn extract_base_command(command: &str) -> Option<&str> {
        command.split_whitespace().next()
    }
}

#[cfg(test)]
mod tests {
    use crate::{
        domain::{config::AppConfigBuilder, package::PackageBuilder},
        ports::{
            command::{MockCommandRunner, MockCommandRunnerExt},
            filesystem::MockFileSystem,
        },
    };

    use super::*;

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

    fn create_installer_deps() -> (MockFileSystem, MockCommandRunner, ProgressManager) {
        (
            MockFileSystem::new(),
            MockCommandRunner::new(),
            ProgressManager::new(false, true, true),
        )
    }

    #[test]
    fn test_installation_manager_install_success() {
        let package = create_test_package();
        let config = create_test_config();
        let (mut fs, mut runner, progress_manager) = create_installer_deps();

        fs.mock_path_exists("/test/path", true);
        fs.mock_path_exists("/test/path/test-package.yaml", true);
        fs.mock_path_exists("/test/path/test-package.yml", false);
        fs.mock_read_file("/test/path/test-package.yaml", package.to_yaml().unwrap());

        runner.error_response("test check", "Not found", 1); // Not installed
        runner.success_response("test install", "Installed successfully");
        runner.mock_is_command_available("test", true);

        let manager = PackageInstaller::new(&fs, &runner, &config, &progress_manager, true);
        let result = manager.install_package(&package.name);

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_installation_manager_already_installed() {
        let package = create_test_package();
        let config = create_test_config();
        let (mut fs, mut runner, progress_manager) = create_installer_deps();

        fs.mock_path_exists("/test/path", true);
        fs.mock_path_exists("/test/path/test-package.yaml", true);
        fs.mock_path_exists("/test/path/test-package.yml", false);
        fs.mock_read_file("/test/path/test-package.yaml", package.to_yaml().unwrap());

        runner.success_response("test check", "Found"); // Already installed
        runner.mock_is_command_available("test", true);

        let manager = PackageInstaller::new(&fs, &runner, &config, &progress_manager, true);
        let result = manager.install_package(&package.name);

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::AlreadyInstalled);
    }

    #[test]
    fn test_installation_manager_install_failure() {
        let package = create_test_package();
        let config = create_test_config();
        let (mut fs, mut runner, progress_manager) = create_installer_deps();

        fs.mock_path_exists("/test/path", true);
        fs.mock_path_exists("/test/path/test-package.yaml", true);
        fs.mock_path_exists("/test/path/test-package.yml", false);
        fs.mock_read_file("/test/path/test-package.yaml", package.to_yaml().unwrap());

        runner.error_response("test check", "Not found", 1); // Not installed
        runner.error_response("test install", "Installation failed", 1);
        runner.mock_is_command_available("test", true);

        let manager = PackageInstaller::new(&fs, &runner, &config, &progress_manager, true);
        let result = manager.install_package(&package.name);

        assert!(result.is_err());
    }

    #[test]
    fn test_installation_manager_environment_incompatible() {
        let package = create_test_package();
        let config = AppConfigBuilder::default()
            .environment("different-env")
            .package_directory("/test/path")
            .build();
        let (mut fs, runner, progress_manager) = create_installer_deps();

        fs.mock_path_exists("/test/path", true);
        fs.mock_path_exists("/test/path/test-package.yaml", true);
        fs.mock_path_exists("/test/path/test-package.yml", false);
        fs.mock_read_file("/test/path/test-package.yaml", package.to_yaml().unwrap());

        let manager = PackageInstaller::new(&fs, &runner, &config, &progress_manager, true);
        let result = manager.install_package(&package.name);

        assert!(result.is_err());
        assert!(
            matches!(
                result,
                Err(PackageInstallerError::DependencyResolverError(
                    DependencyResolverError::EnvironmentNotSupported(_, _)
                )),
            ),
            "{:#?}",
            result
        );
    }
}
