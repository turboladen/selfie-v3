// src/services/multi_package_installation_service.rs
mod dependency;

use std::time::{Duration, Instant};

use console::style;
use thiserror::Error;

use crate::{
    adapters::{
        package_repo::yaml::YamlPackageRepository,
        progress::{ProgressManager, ProgressStyleType},
    },
    domain::{
        config::Config,
        dependency::DependencyGraphError,
        installation::{InstallationError, InstallationStatus},
        package::Package,
    },
    ports::{
        command::{CommandError, CommandOutput, CommandRunner},
        filesystem::{FileSystem, FileSystemError},
        package_repo::{PackageRepoError, PackageRepository},
    },
    services::{
        command_validator::CommandValidator,
        package_installation_service::PackageInstallationService,
    },
};

use self::dependency::{DependencyResolver, DependencyResolverError};

#[derive(Error, Debug)]
pub enum PackageInstallerError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("FileSystem error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("Package repository error: {0}")]
    PackageRepoError(#[from] PackageRepoError),

    #[error("Dependency error: {0}")]
    DependencyError(DependencyResolverError),

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

// Implementation to convert DependencyResolverError to PackageInstallerError
impl From<DependencyResolverError> for PackageInstallerError {
    fn from(err: DependencyResolverError) -> Self {
        match err {
            DependencyResolverError::CircularDependency(cycle) => {
                PackageInstallerError::CircularDependency(cycle)
            }
            DependencyResolverError::PackageNotFound(pkg) => {
                PackageInstallerError::PackageNotFound(pkg)
            }
            DependencyResolverError::MultiplePackagesFound(pkg) => {
                PackageInstallerError::MultiplePackagesFound(pkg)
            }
            DependencyResolverError::RepoError(e) => PackageInstallerError::PackageRepoError(e),
            DependencyResolverError::GraphError(e) => match e {
                DependencyGraphError::CircularDependency(msg, path) => {
                    PackageInstallerError::CircularDependency(format!(
                        "{} ({})",
                        msg,
                        path.join(", ")
                    ))
                }
                DependencyGraphError::PackageNotFound(pkg) => {
                    PackageInstallerError::PackageNotFound(pkg)
                }
                _ => PackageInstallerError::DependencyError(DependencyResolverError::GraphError(e)),
            },
            _ => PackageInstallerError::DependencyError(err),
        }
    }
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

pub struct MultiPackageInstallationService<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    runner: &'a R,
    config: &'a Config,
    progress_manager: ProgressManager,
    verbose: bool,
    check_commands: bool, // New flag to enable command availability checking
}

impl<'a, F: FileSystem, R: CommandRunner> MultiPackageInstallationService<'a, F, R> {
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: &'a Config,
        verbose: bool,
        use_colors: bool,
        use_unicode: bool,
        check_commands: bool, // Added parameter
    ) -> Self {
        let progress_manager = ProgressManager::new(use_colors, use_unicode, verbose);

        Self {
            fs,
            runner,
            config,
            progress_manager,
            verbose,
            check_commands,
        }
    }

    /// Install a package by name with enhanced progress reporting
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

        // Create dependency resolver
        let package_repo =
            YamlPackageRepository::new(self.fs, self.config.expanded_package_directory());
        let resolver = DependencyResolver::new(package_repo, self.config);

        // Update progress
        main_progress.set_message("Resolving dependencies...");

        // Start timing the entire process
        let start_time = Instant::now();

        // Resolve dependencies - handle circular dependency errors specially
        let packages = match resolver.resolve_dependencies(package_name) {
            Ok(packages) => packages,
            Err(err) => {
                // Handle the error case
                let error_message = match &err {
                    DependencyResolverError::CircularDependency(cycle) => {
                        // Format a more user-friendly circular dependency error
                        if self.progress_manager.use_colors() {
                            format!(
                                "Circular dependency detected: {}",
                                style(cycle).red().bold()
                            )
                        } else {
                            format!("Circular dependency detected: {}", cycle)
                        }
                    }
                    _ => format!("{}", err),
                };

                // Update the progress bar with the error
                main_progress.abandon_with_message("Dependency resolution failed");

                // Print the specific error message
                eprintln!("Error: {}", error_message);

                // Include more details in verbose mode
                if self.verbose {
                    eprintln!("\nInstallation cannot proceed with circular dependencies.");
                    eprintln!("Please fix the circular dependency in your package definitions.");
                }

                // Return the error
                return Err(err.into());
            }
        };

        // Pre-flight check: check if all required commands are available if check_commands is true
        if self.check_commands {
            let command_validator = CommandValidator::new(self.runner);
            let mut missing_commands = Vec::new();

            // Check commands for each package
            for package in &packages {
                if let Some(env_config) = package.environments.get(&self.config.environment) {
                    // Extract and check base command
                    if let Some(base_cmd) =
                        CommandValidator::<R>::extract_base_command(&env_config.install)
                    {
                        if !self.runner.is_command_available(base_cmd) {
                            missing_commands.push((package.name.clone(), base_cmd.to_string()));
                        }
                    }
                }
            }

            // If any commands are missing, report and exit
            if !missing_commands.is_empty() {
                main_progress.abandon_with_message("Command availability check failed");

                let mut error_msg = String::from(
                    "The following commands required for installation are not available:\n\n",
                );
                for (pkg, cmd) in missing_commands {
                    error_msg.push_str(&format!("  • Package '{}' requires '{}'\n", pkg, cmd));
                }

                error_msg.push_str("\nPlease install these commands and try again.");
                return Err(PackageInstallerError::CommandNotAvailable(error_msg));
            }
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
        if !package.environments.contains_key(&self.config.environment) {
            return Ok(false);
        }

        // Check if required commands are available
        if self.check_commands {
            let env_config = package.environments.get(&self.config.environment).unwrap();
            if let Some(base_cmd) = CommandValidator::<R>::extract_base_command(&env_config.install)
            {
                if !self.runner.is_command_available(base_cmd) {
                    return Ok(false);
                }
            }
        }

        // Check dependencies if requested
        let resolver = DependencyResolver::new(package_repo, self.config);
        match resolver.resolve_dependencies(package_name) {
            Ok(_) => Ok(true),
            Err(DependencyResolverError::CircularDependency(_)) => Ok(false),
            Err(DependencyResolverError::PackageNotFound(_)) => Ok(false),
            Err(DependencyResolverError::EnvironmentNotSupported(_, _)) => Ok(false),
            Err(e) => Err(e.into()),
        }
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
            if let Some(env_config) = package.environments.get(&self.config.environment) {
                if let Some(base_cmd) =
                    CommandValidator::<R>::extract_base_command(&env_config.install)
                {
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

        // Create installation manager
        let installation_manager = PackageInstallationService::new(self.runner, self.config);

        // Update progress status to checking
        self.progress_manager
            .update_from_status(progress_id, &InstallationStatus::Checking, None)
            .map_err(PackageInstallerError::EnvironmentError)?;

        // Short delay to allow the spinner to visibly show checking state
        std::thread::sleep(Duration::from_millis(200));

        // Install the package
        let result = match installation_manager.install_package(package.clone()) {
            Ok(installation) => {
                let duration = start_time.elapsed();

                match installation.status {
                    InstallationStatus::AlreadyInstalled => {
                        // For already installed packages, update the progress bar
                        // with the status, but don't repeat it in the final status
                        self.progress_manager
                            .update_from_status(progress_id, &installation.status, Some(duration))
                            .map_err(PackageInstallerError::EnvironmentError)?;

                        // Return the result directly without further status messages
                        InstallationResult::already_installed(&package.name, duration)
                    }
                    InstallationStatus::Complete => {
                        // For successfully installed packages, update progress
                        self.progress_manager
                            .update_from_status(progress_id, &installation.status, Some(duration))
                            .map_err(PackageInstallerError::EnvironmentError)?;

                        // Return the successful result
                        InstallationResult::success(&package.name, duration, None)
                    }
                    _ => {
                        // For other states (usually errors), update progress
                        self.progress_manager
                            .update_from_status(progress_id, &installation.status, Some(duration))
                            .map_err(PackageInstallerError::EnvironmentError)?;

                        // Return error result
                        let error_msg = format!("Installation failed: {:?}", installation.status);
                        InstallationResult::failed(
                            &package.name,
                            InstallationStatus::Failed(error_msg.clone()),
                            duration,
                        )
                    }
                }
            }
            Err(err) => {
                let duration = start_time.elapsed();
                let error_msg = format!("Installation error: {}", err);

                // Update progress bar with error
                self.progress_manager
                    .update_from_status(
                        progress_id,
                        &InstallationStatus::Failed(err.to_string()),
                        Some(duration),
                    )
                    .map_err(PackageInstallerError::EnvironmentError)?;

                // Return error result
                InstallationResult::failed(
                    &package.name,
                    InstallationStatus::Failed(error_msg.clone()),
                    duration,
                )
            }
        };

        // Small delay to ensure the final status is visible
        std::thread::sleep(Duration::from_millis(100));

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
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        domain::config::ConfigBuilder,
        ports::command::{MockCommandRunner, MockCommandRunnerExt},
        ports::filesystem::MockFileSystem,
    };
    use std::path::Path;

    fn create_test_config() -> Config {
        ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build()
    }

    fn create_test_environment() -> (MockFileSystem, MockCommandRunner, Config) {
        let fs = MockFileSystem::default();
        let runner = MockCommandRunner::new();
        let config = create_test_config();
        (fs, runner, config)
    }

    #[test]
    fn test_install_package_success() {
        let (mut fs, mut runner, config) = create_test_environment();

        // Set up the package file
        let package_yaml = r#"
            name: ripgrep
            version: 1.0.0
            environments:
              test-env:
                install: test install
                check: test check
        "#;

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(&package_dir, true);

        let ripgrep = package_dir.join("ripgrep.yaml");
        fs.mock_path_exists(&ripgrep, true);
        fs.mock_path_exists(package_dir.join("ripgrep.yml"), false);
        fs.mock_read_file(&ripgrep, package_yaml);

        // Set up mock command responses
        runner.error_response("test check", "Not found", 1); // Not installed
        runner.success_response("test install", "Installed successfully");

        // Make base command available
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("test"))
            .returning(|_| true);

        // Create the installer with command checking enabled
        let installer =
            MultiPackageInstallationService::new(&fs, &runner, &config, false, false, false, true);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_install_package_command_not_available() {
        let (mut fs, mut runner, config) = create_test_environment();

        // Set up the package file
        let package_yaml = r#"
            name: ripgrep
            version: 1.0.0
            environments:
              test-env:
                install: missing-cmd install
                check: missing-cmd check
        "#;

        let package_dir = Path::new("/test/packages");
        let package = package_dir.join("ripgrep.yaml");
        fs.mock_path_exists(&package_dir, true);
        fs.mock_path_exists(&package, true);
        fs.mock_path_exists(package_dir.join("ripgrep.yml"), false);
        fs.mock_read_file(&package, package_yaml);

        // Make base command unavailable
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("missing-cmd"))
            .returning(|_| false);

        // Create the installer with command checking enabled
        let installer =
            MultiPackageInstallationService::new(&fs, &runner, &config, false, false, false, true);

        // Run the installation - should fail due to missing command
        let result = installer.install_package("ripgrep");
        assert!(result.is_err());

        // Verify it's the right error type
        match result {
            Err(PackageInstallerError::CommandNotAvailable(_)) => {
                // This is the expected error
            }
            _ => panic!("Expected CommandNotAvailable error but got {:?}", result),
        }
    }

    #[test]
    fn test_check_package_installable() {
        let (mut fs, mut runner, config) = create_test_environment();

        // Set up multiple package files
        let package1_yaml = r#"
            name: available
            version: 1.0.0
            environments:
              test-env:
                install: available-cmd install
        "#;

        let package2_yaml = r#"
            name: wrong-env
            version: 1.0.0
            environments:
              other-env:
                install: some-cmd install
        "#;

        let package3_yaml = r#"
            name: missing-cmd
            version: 1.0.0
            environments:
              test-env:
                install: missing-cmd install
        "#;

        let package_dir = Path::new("/test/packages");
        let available = package_dir.join("available.yaml");
        let wrong_env = package_dir.join("wrong-env.yaml");
        let missing_cmd = package_dir.join("missing-cmd.yaml");
        fs.mock_path_exists(&package_dir, true);
        fs.mock_path_exists(&available, true);
        fs.mock_path_exists(package_dir.join("available.yml"), false);
        fs.mock_path_exists(&wrong_env, true);
        fs.mock_path_exists(package_dir.join("wrong-env.yml"), false);
        fs.mock_path_exists(&missing_cmd, true);
        fs.mock_path_exists(package_dir.join("missing-cmd.yml"), false);
        fs.mock_path_exists(package_dir.join("nonexistent.yaml"), false);
        fs.mock_path_exists(package_dir.join("nonexistent.yml"), false);

        fs.mock_read_file(&available, package1_yaml);
        fs.mock_read_file(&wrong_env, package2_yaml);
        fs.mock_read_file(&missing_cmd, package3_yaml);

        // Set up command availability
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("available-cmd"))
            .returning(|_| true);
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("some-cmd"))
            .returning(|_| true);
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("missing-cmd"))
            .returning(|_| false);

        // Create the installer with command checking enabled
        let installer =
            MultiPackageInstallationService::new(&fs, &runner, &config, false, false, false, true);

        // Package with available command in correct environment should be installable
        let result = installer.check_package_installable("available");
        assert!(result.is_ok());
        assert!(result.unwrap());

        // Package with wrong environment should not be installable
        let result = installer.check_package_installable("wrong-env");
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Package with missing command should not be installable
        let result = installer.check_package_installable("missing-cmd");
        assert!(result.is_ok());
        assert!(!result.unwrap());

        // Non-existent package should return error
        let result = installer.check_package_installable("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_install_package_with_dependencies() {
        let (mut fs, mut runner, config) = create_test_environment();

        // Set up the main package file with dependencies
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

        // Set up the dependency package file
        let dependency_yaml = r#"
            name: rust
            version: 1.0.0
            environments:
              test-env:
                install: rust install
                check: rust check
        "#;

        let package_dir = Path::new("/test/packages");
        let ripgrep = package_dir.join("ripgrep.yaml");
        let rust = package_dir.join("rust.yaml");
        fs.mock_path_exists(&package_dir, true);
        fs.mock_path_exists(&ripgrep, true);
        fs.mock_path_exists(package_dir.join("ripgrep.yml"), false);
        fs.mock_path_exists(&rust, true);
        fs.mock_path_exists(package_dir.join("rust.yml"), false);

        fs.mock_read_file(&ripgrep, package_yaml);
        fs.mock_read_file(&rust, dependency_yaml);

        // Set up mock command responses
        runner.error_response("rg check", "Not found", 1); // Not installed
        runner.success_response("rg install", "Installed successfully");
        runner.error_response("rust check", "Not found", 1); // Not installed
        runner.success_response("rust install", "Installed successfully");

        // Make commands available
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("rg"))
            .returning(|_| true);
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("rust"))
            .returning(|_| true);

        // Create the installer with command checking enabled
        let installer =
            MultiPackageInstallationService::new(&fs, &runner, &config, false, false, false, true);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.status, InstallationStatus::Complete);
        assert_eq!(install_result.dependencies.len(), 1);
        assert_eq!(install_result.dependencies[0].package_name, "rust");
        assert_eq!(
            install_result.dependencies[0].status,
            InstallationStatus::Complete
        );
    }

    #[test]
    fn test_install_package_with_dependency_command_not_available() {
        let (mut fs, mut runner, config) = create_test_environment();

        // Set up the main package file with dependencies
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

        // Set up the dependency package file with a command that won't be available
        let dependency_yaml = r#"
            name: rust
            version: 1.0.0
            environments:
              test-env:
                install: unavailable-cmd install
                check: unavailable-cmd check
        "#;

        let package_dir = Path::new("/test/packages");
        let ripgrep = package_dir.join("ripgrep.yaml");
        let rust = package_dir.join("rust.yaml");
        fs.mock_path_exists(&package_dir, true);
        fs.mock_path_exists(&ripgrep, true);
        fs.mock_path_exists(package_dir.join("ripgrep.yml"), false);
        fs.mock_path_exists(&rust, true);
        fs.mock_path_exists(package_dir.join("rust.yml"), false);

        fs.mock_read_file(&ripgrep, package_yaml);
        fs.mock_read_file(&rust, dependency_yaml);

        // Set up command availability
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("rg"))
            .returning(|_| true);
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("unavailable-cmd"))
            .returning(|_| false);

        // Create the installer with command checking enabled
        let installer =
            MultiPackageInstallationService::new(&fs, &runner, &config, false, false, false, true);

        // Run the installation - should fail due to missing dependency command
        let result = installer.install_package("ripgrep");
        assert!(result.is_err());

        // Verify it's the right error type
        match result {
            Err(PackageInstallerError::CommandNotAvailable(_)) => {
                // This is the expected error
            }
            _ => panic!("Expected CommandNotAvailable error but got {:?}", result),
        }
    }
}
