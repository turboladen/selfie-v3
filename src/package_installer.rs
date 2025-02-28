// src/package_installer.rs
mod dependency;

use std::time::{Duration, Instant};

use console::style;
use thiserror::Error;

use crate::{
    command::{CommandError, CommandOutput, CommandRunner},
    config::Config,
    filesystem::{FileSystem, FileSystemError},
    graph::DependencyGraphError,
    installation::{InstallationError, InstallationManager, InstallationStatus},
    package::PackageNode,
    package_repo::PackageRepoError,
    progress_display::{ProgressManager, ProgressStyleType},
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
                    PackageInstallerError::CircularDependency(msg.to_string())
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

pub struct PackageInstaller<F: FileSystem, R: CommandRunner + Clone> {
    fs: F,
    runner: R,
    config: Config,
    progress_manager: ProgressManager,
    verbose: bool,
}

impl<F: FileSystem, R: CommandRunner + Clone> PackageInstaller<F, R> {
    pub fn new(
        fs: F,
        runner: R,
        config: Config,
        verbose: bool,
        use_colors: bool,
        use_unicode: bool,
    ) -> Self {
        let progress_manager = ProgressManager::new(use_colors, use_unicode, verbose);

        Self {
            fs,
            runner,
            config,
            progress_manager,
            verbose,
        }
    }

    /// Install a package by name with enhanced progress reporting
    pub async fn install_package(
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
        let resolver = DependencyResolver::new(&self.fs, &self.config);

        // Update progress
        main_progress.set_message("Resolving dependencies...");

        // Start timing the entire process
        let start_time = Instant::now();

        // Resolve dependencies - handle circular dependency errors specially
        let packages = match resolver.resolve_dependencies(package_name).await {
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
            match self.install_single_package(package, &dep_id).await {
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

        let main_result = self.install_single_package(&main_package, &main_id).await?;

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

    /// Install a single package (no dependency handling) with progress reporting
    async fn install_single_package(
        &self,
        package: &PackageNode,
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

        // Create installation manager
        let installation_manager =
            InstallationManager::new(self.runner.clone(), self.config.clone());

        // Update progress status to checking
        self.progress_manager
            .update_from_status(progress_id, &InstallationStatus::Checking, None)
            .map_err(PackageInstallerError::EnvironmentError)?;

        // Short delay to allow the spinner to visibly show checking state
        tokio::time::sleep(Duration::from_millis(200)).await;

        // Install the package
        let result = match installation_manager.install_package(package.clone()).await {
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
        tokio::time::sleep(Duration::from_millis(100)).await;

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
        command::mock::MockCommandRunner,
        config::ConfigBuilder,
        filesystem::mock::MockFileSystem,
        progress::{ConsoleRenderer, ProgressReporter},
    };

    fn create_test_config() -> Config {
        ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build()
    }

    fn create_test_environment() -> (MockFileSystem, MockCommandRunner, ProgressReporter, Config) {
        let fs = MockFileSystem::default();
        let runner = MockCommandRunner::new();
        let reporter = ProgressReporter::new(Box::new(ConsoleRenderer::new(false, false)));
        let config = create_test_config();
        (fs, runner, reporter, config)
    }

    #[tokio::test]
    async fn test_install_package_success() {
        let (fs, runner, _, config) = create_test_environment();

        // Set up the package file
        let package_yaml = r#"
            name: ripgrep
            version: 1.0.0
            environments:
              test-env:
                install: test install
                check: test check
        "#;

        fs.add_file(
            std::path::Path::new("/test/packages/ripgrep.yaml"),
            package_yaml,
        );
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Set up mock command responses
        runner.error_response("test check", "Not found", 1); // Not installed
        runner.success_response("test install", "Installed successfully");

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, false, false, false);

        // Run the installation
        let result = installer.install_package("ripgrep").await;

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.status, InstallationStatus::Complete);
    }

    #[tokio::test]
    async fn test_install_package_already_installed() {
        let (fs, runner, _, config) = create_test_environment();

        // Set up the package file
        let package_yaml = r#"
            name: ripgrep
            version: 1.0.0
            environments:
              test-env:
                install: test install
                check: test check
        "#;

        fs.add_file(
            std::path::Path::new("/test/packages/ripgrep.yaml"),
            package_yaml,
        );
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Set up mock command responses
        runner.success_response("test check", "Found"); // Already installed

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, false, false, false);

        // Run the installation
        let result = installer.install_package("ripgrep").await;

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.status, InstallationStatus::AlreadyInstalled);
    }

    #[tokio::test]
    async fn test_install_package_with_dependencies() {
        let (fs, runner, _, config) = create_test_environment();

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

        fs.add_file(
            std::path::Path::new("/test/packages/ripgrep.yaml"),
            package_yaml,
        );
        fs.add_file(
            std::path::Path::new("/test/packages/rust.yaml"),
            dependency_yaml,
        );
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Set up mock command responses
        runner.error_response("rg check", "Not found", 1); // Not installed
        runner.success_response("rg install", "Installed successfully");
        runner.error_response("rust check", "Not found", 1); // Not installed
        runner.success_response("rust install", "Installed successfully");

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, false, false, false);

        // Run the installation
        let result = installer.install_package("ripgrep").await;

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

    #[tokio::test]
    async fn test_install_package_with_failing_dependency() {
        let (fs, runner, _, config) = create_test_environment();

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

        fs.add_file(
            std::path::Path::new("/test/packages/ripgrep.yaml"),
            package_yaml,
        );
        fs.add_file(
            std::path::Path::new("/test/packages/rust.yaml"),
            dependency_yaml,
        );
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Set up mock command responses - make the dependency installation fail
        runner.error_response("rust check", "Not found", 1); // Not installed
        runner.error_response("rust install", "Installation failed", 1); // Installation fails

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, false, false, false);

        // Run the installation
        let result = installer.install_package("ripgrep").await;

        // Verify the result - we explicitly expect an error of type InstallationError
        assert!(result.is_err());
        match result {
            Err(PackageInstallerError::InstallationError(_)) => (), // This is what we expect
            other => panic!("Expected InstallationError, got {:?}", other),
        }
    }

    #[tokio::test]
    async fn test_complex_dependency_chain() {
        let (fs, runner, _, config) = create_test_environment();

        // Set up package files with a dependency chain: main-pkg -> dep1 -> dep2
        let main_pkg_yaml = r#"
            name: main-pkg
            version: 1.0.0
            environments:
              test-env:
                install: main-install
                check: main-check
                dependencies:
                  - dep1
        "#;

        let dep1_yaml = r#"
            name: dep1
            version: 1.0.0
            environments:
              test-env:
                install: dep1-install
                check: dep1-check
                dependencies:
                  - dep2
        "#;

        let dep2_yaml = r#"
            name: dep2
            version: 1.0.0
            environments:
              test-env:
                install: dep2-install
                check: dep2-check
        "#;

        fs.add_file(
            std::path::Path::new("/test/packages/main-pkg.yaml"),
            main_pkg_yaml,
        );
        fs.add_file(std::path::Path::new("/test/packages/dep1.yaml"), dep1_yaml);
        fs.add_file(std::path::Path::new("/test/packages/dep2.yaml"), dep2_yaml);
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Set up mock command responses
        runner.error_response("main-check", "Not found", 1);
        runner.success_response("main-install", "Installed successfully");
        runner.error_response("dep1-check", "Not found", 1);
        runner.success_response("dep1-install", "Installed successfully");
        runner.error_response("dep2-check", "Not found", 1);
        runner.success_response("dep2-install", "Installed successfully");

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, false, false, false);

        // Run the installation
        let result = installer.install_package("main-pkg").await;

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "main-pkg");
        assert_eq!(install_result.status, InstallationStatus::Complete);
        assert_eq!(install_result.dependencies.len(), 2);

        // Find dep1 and dep2 in dependencies
        let has_dep1 = install_result
            .dependencies
            .iter()
            .any(|d| d.package_name == "dep1");
        let has_dep2 = install_result
            .dependencies
            .iter()
            .any(|d| d.package_name == "dep2");

        assert!(has_dep1);
        assert!(has_dep2);
    }
}
