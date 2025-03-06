// src/services/package_installer.rs
mod dependency;

use console::style;
use dependency::{DependencyResolver, DependencyResolverError};
use std::time::{Duration, Instant};
use thiserror::Error;

use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
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
        // Create package repository
        let package_repo =
            YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());

        // Start timing the entire process
        let start_time = Instant::now();

        // First, find the package to get its details
        let main_package = package_repo
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

        // Get the package file path to display
        let package_path = if let Some(path) = &main_package.path {
            path.to_string_lossy().to_string()
        } else {
            format!(
                "{}/{}.yaml",
                self.config.expanded_package_directory().display(),
                package_name
            )
        };

        // Print initial package info header
        let header = if self.progress_manager.use_colors() {
            format!(
                "Installing {} (v{}) from {}",
                style(&main_package.name).magenta().bold(),
                main_package.version,
                package_path
            )
        } else {
            format!(
                "Installing {} (v{}) from {}",
                main_package.name, main_package.version, package_path
            )
        };
        println!("{}", header);

        // Resolve dependencies without any progress bars
        let packages = match self.resolve_dependencies(package_name, &package_repo) {
            Ok(packages) => packages,
            Err(err) => {
                println!("Dependency resolution failed: {}", err);
                return Err(err);
            }
        };

        // Pre-flight check: check if all required commands are available
        if self.check_commands && !self.verify_commands(&packages)? {
            return Err(PackageInstallerError::CommandNotAvailable(
                "Required commands not available".to_string(),
            ));
        }

        // Install all packages in order
        let mut dependency_results = Vec::new();

        // Show dependency section if we have dependencies
        if packages.len() > 1 {
            println!("  Dependencies:");

            // All packages except the last one are dependencies
            for package in packages.iter().take(packages.len() - 1) {
                let package_path = if let Some(path) = &package.path {
                    path.to_string_lossy().to_string()
                } else {
                    format!(
                        "{}/{}.yaml",
                        self.config.expanded_package_directory().display(),
                        package.name
                    )
                };

                // Show dependency name and version
                let dependency_header = if self.progress_manager.use_colors() {
                    format!(
                        "    Installing {} (v{}) from {}",
                        style(&package.name).magenta(),
                        package.version,
                        package_path
                    )
                } else {
                    format!(
                        "    Installing {} (v{}) from {}",
                        package.name, package.version, package_path
                    )
                };
                println!("{}", dependency_header);

                // Install the dependency
                match self.install_single_package(package, 6) {
                    Ok(result) => {
                        // Only continue if installation was successful or package was already installed
                        match result.status {
                            InstallationStatus::Complete | InstallationStatus::AlreadyInstalled => {
                                dependency_results.push(result);
                            }
                            _ => {
                                println!("      ✗ Dependency installation failed");
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
                        println!(
                            "      ✗ Failed to install dependency '{}': {}",
                            package.name, err
                        );
                        return Err(err);
                    }
                }
            }
        }

        // Now install the main package
        let main_package = packages.last().unwrap();
        let main_result = self.install_single_package(main_package, 2)?;

        // Get the total installation time and create the final result
        let total_duration = start_time.elapsed();
        let mut final_result = main_result.with_dependencies(dependency_results);

        // Override the duration with the main package duration only
        final_result.duration = total_duration;

        // Print summary
        println!();
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
    fn verify_commands(&self, packages: &[Package]) -> Result<bool, PackageInstallerError> {
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
        indent_level: usize,
    ) -> Result<InstallationResult, PackageInstallerError> {
        let start_time = Instant::now();
        let indent = " ".repeat(indent_level);

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

        // Start the installation process
        installation.start();

        // Check if already installed
        let already_installed = installation
            .execute_check(self.runner)
            .map_err(PackageInstallerError::InstallationError)?;

        let check_duration = start_time.elapsed();

        // Display the result of the check
        if already_installed {
            installation.complete(InstallationStatus::AlreadyInstalled);

            // Print "Already installed" with duration
            let status_message = format!(
                "{}✓ Checking installation status: Already installed ({:.1?})",
                indent, check_duration
            );
            println!("{}", status_message);

            return Ok(InstallationResult::already_installed(
                &package.name,
                check_duration,
            ));
        } else {
            // Print "Not installed" with duration
            let status_message = format!(
                "{}✓ Checking installation status: Not installed ({:.1?})",
                indent, check_duration
            );
            println!("{}", status_message);
        }

        // Print installing message
        println!("{}⌛ Installing...", indent);

        // Execute installation
        let result = match installation.execute_install(self.runner) {
            Ok(output) => {
                let duration = start_time.elapsed();
                installation.complete(InstallationStatus::Complete);

                // Print completion message with duration
                let complete_message =
                    format!("{}✓ Installation complete ({:.1?})", indent, duration);
                println!("{}", complete_message);

                InstallationResult::success(&package.name, duration, Some(output))
            }
            Err(err) => {
                let duration = start_time.elapsed();
                installation.update_status(InstallationStatus::Failed(format!(
                    "Installation error: {}",
                    err
                )));

                // Print error message
                let error_message = format!(
                    "{}✗ Installation failed: {} ({:.1?})",
                    indent, err, duration
                );
                println!("{}", error_message);

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

        // Display timing information according to spec
        if !result.dependencies.is_empty() {
            println!("Total time: {:.1?}", total_duration);
            println!("Dependencies: {:.1?}", dep_duration);
            println!("Package: {:.1?}", package_duration);
        } else {
            println!("Total time: {:.1?}", total_duration);
        }
    }

    /// Extract the base command from a command string
    fn extract_base_command(command: &str) -> Option<&str> {
        command.split_whitespace().next()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::{config::AppConfigBuilder, package::PackageBuilder},
        ports::{
            command::{MockCommandRunner, MockCommandRunnerExt},
            filesystem::MockFileSystem,
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
