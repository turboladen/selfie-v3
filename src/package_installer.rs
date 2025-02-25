// src/package_installer.rs

use std::time::{Duration, Instant};

use thiserror::Error;

use crate::command::{CommandError, CommandOutput, CommandRunner};
use crate::config::Config;
use crate::filesystem::{FileSystem, FileSystemError};
use crate::graph::DependencyGraphError;
use crate::installation::{InstallationError, InstallationManager, InstallationStatus};
use crate::package::PackageNode;
use crate::package_repo::{PackageRepoError, PackageRepository};
use crate::progress::ProgressReporter;

#[derive(Error, Debug)]
pub enum PackageInstallerError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("FileSystem error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("Package repository error: {0}")]
    PackageRepoError(#[from] PackageRepoError),

    #[error("Dependency error: {0}")]
    DependencyError(#[from] DependencyGraphError),

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
    reporter: ProgressReporter,
    verbose: bool,
}

impl<F: FileSystem, R: CommandRunner + Clone> PackageInstaller<F, R> {
    pub fn new(
        fs: F,
        runner: R,
        config: Config,
        reporter: ProgressReporter,
        verbose: bool,
    ) -> Self {
        Self {
            fs,
            runner,
            config,
            reporter,
            verbose,
        }
    }

    /// Install a package by name
    pub fn install_package(
        &self,
        package_name: &str,
    ) -> Result<InstallationResult, PackageInstallerError> {
        // Report progress - beginning installation
        println!(
            "{}",
            self.reporter
                .info(&format!("Installing package '{}'", package_name))
        );

        // Get the package from the repository
        let start_time = Instant::now();
        let repo = PackageRepository::new(&self.fs, self.config.expanded_package_directory());
        let package = repo.get_package(package_name).map_err(|e| match e {
            PackageRepoError::PackageNotFound(s) => PackageInstallerError::PackageNotFound(s),
            PackageRepoError::MultiplePackagesFound(s) => {
                PackageInstallerError::MultiplePackagesFound(s)
            }
            t => PackageInstallerError::PackageRepoError(t),
        })?;

        // Build and resolve the dependency graph
        let dependencies = self.install_dependencies(&package)?;

        // Create installation manager
        let installation_manager =
            InstallationManager::new(self.runner.clone(), self.config.clone());

        // Install the package
        println!(
            "{}",
            self.reporter.loading(&format!(
                "Installing {} (v{})",
                package.name, package.version
            ))
        );

        let result = match installation_manager.install_package(package.clone()) {
            Ok(installation) => {
                let duration = start_time.elapsed();
                match installation.status {
                    InstallationStatus::Complete => {
                        println!(
                            "{}",
                            self.reporter
                                .success(&format!("Installation complete ({:.1?})", duration))
                        );
                        InstallationResult::success(&package.name, duration, None)
                    }
                    InstallationStatus::AlreadyInstalled => {
                        println!(
                            "{}",
                            self.reporter
                                .success(&format!("Package already installed ({:.1?})", duration))
                        );
                        InstallationResult::already_installed(&package.name, duration)
                    }
                    _ => {
                        let error_msg = format!("Installation failed: {:?}", installation.status);
                        println!("{}", self.reporter.error(&error_msg));
                        InstallationResult::failed(&package.name, installation.status, duration)
                    }
                }
            }
            Err(err) => {
                let duration = start_time.elapsed();
                let error_msg = format!("Installation error: {}", err);
                println!("{}", self.reporter.error(&error_msg));
                InstallationResult::failed(
                    &package.name,
                    InstallationStatus::Failed(err.to_string()),
                    duration,
                )
            }
        };

        // Add dependencies to the result
        let result = result.with_dependencies(dependencies);

        // Report final status including timing information
        self.report_final_status(&result);

        Ok(result)
    }

    /// Install all dependencies for a package
    fn install_dependencies(
        &self,
        package: &PackageNode,
    ) -> Result<Vec<InstallationResult>, PackageInstallerError> {
        // Resolve environment-specific dependencies
        let env_config = self
            .config
            .resolve_environment(package)
            .map_err(|e| PackageInstallerError::EnvironmentError(e.to_string()))?;

        if env_config.dependencies.is_empty() {
            return Ok(Vec::new());
        }

        println!("{}", self.reporter.info("  Dependencies:"));

        let mut results = Vec::new();
        let repo = PackageRepository::new(&self.fs, self.config.expanded_package_directory());

        // Install each dependency directly instead of creating a new installer
        for dep_name in &env_config.dependencies {
            println!(
                "{}",
                self.reporter
                    .info(&format!("    Installing dependency '{}'", dep_name))
            );

            // Get the dependency package
            let dep_package = repo.get_package(dep_name).map_err(|e| match e {
                PackageRepoError::PackageNotFound(s) => PackageInstallerError::PackageNotFound(s),
                PackageRepoError::MultiplePackagesFound(s) => {
                    PackageInstallerError::MultiplePackagesFound(s)
                }
                t => PackageInstallerError::PackageRepoError(t),
            })?;

            // Create installation manager
            let installation_manager =
                InstallationManager::new(self.runner.clone(), self.config.clone());

            // Install the dependency
            let start_time = std::time::Instant::now();

            let result = match installation_manager.install_package(dep_package.clone()) {
                Ok(installation) => {
                    let duration = start_time.elapsed();
                    match installation.status {
                        InstallationStatus::Complete => {
                            println!(
                                "{}",
                                self.reporter.success(&format!(
                                    "    Dependency installation complete ({:.1?})",
                                    duration
                                ))
                            );
                            InstallationResult::success(dep_name, duration, None)
                        }
                        InstallationStatus::AlreadyInstalled => {
                            println!(
                                "{}",
                                self.reporter.success(&format!(
                                    "    Dependency already installed ({:.1?})",
                                    duration
                                ))
                            );
                            InstallationResult::already_installed(dep_name, duration)
                        }
                        _ => {
                            let error_msg = format!(
                                "Dependency installation failed: {:?}",
                                installation.status
                            );
                            println!("{}", self.reporter.error(&error_msg));
                            return Err(PackageInstallerError::DependencyError(
                                DependencyGraphError::InvalidDependency(format!(
                                    "Failed to install dependency '{}': {}",
                                    dep_name, error_msg
                                )),
                            ));
                        }
                    }
                }
                Err(err) => {
                    let error_msg = format!("Dependency installation error: {}", err);
                    println!("{}", self.reporter.error(&error_msg));
                    return Err(PackageInstallerError::DependencyError(
                        DependencyGraphError::InvalidDependency(format!(
                            "Failed to install dependency '{}': {}",
                            dep_name, err
                        )),
                    ));
                }
            };

            results.push(result);
        }

        Ok(results)
    }

    /// Report the final installation status with timing information
    fn report_final_status(&self, result: &InstallationResult) {
        let total_duration = result.total_duration();
        let dep_duration = result.dependency_duration();
        let package_duration = result.duration;

        if !result.dependencies.is_empty() {
            println!("\nTotal time: {:.1?}", total_duration);
            println!("Dependencies: {:.1?}", dep_duration);
            println!("Package: {:.1?}", package_duration);
        } else {
            println!("Total time: {:.1?}", total_duration);
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;

    use crate::{
        command::mock::MockCommandRunner,
        config::ConfigBuilder,
        filesystem::mock::MockFileSystem,
        package::PackageNodeBuilder,
        progress::ConsoleRenderer,
    };

    fn create_test_package(name: &str) -> PackageNode {
        PackageNodeBuilder::default()
            .name(name)
            .version("1.0.0")
            .environment_with_check("test-env", "test install", "test check")
            .build()
    }

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

    #[test]
    fn test_install_package_success() {
        let (fs, runner, reporter, config) = create_test_environment();

        // Set up the package file
        let package = create_test_package("ripgrep");
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
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.status, InstallationStatus::Complete);
    }

    #[test]
    fn test_install_package_already_installed() {
        let (fs, runner, reporter, config) = create_test_environment();

        // Set up the package file
        let package = create_test_package("ripgrep");
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
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        assert_eq!(install_result.status, InstallationStatus::AlreadyInstalled);
    }

    #[test]
    fn test_install_package_not_found() {
        let (fs, runner, reporter, config) = create_test_environment();

        // Don't add any package files
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("nonexistent");

        // Verify the result
        assert!(result.is_err());
        match result {
            Err(PackageInstallerError::PackageNotFound(_)) => (),
            t => panic!("Expected PackageNotFound error; got {t:#?}"),
        }
    }

    #[test]
    fn test_install_package_with_dependencies() {
        let (fs, runner, reporter, config) = create_test_environment();

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
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

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
    fn test_install_package_with_failing_dependency() {
        let (fs, runner, reporter, config) = create_test_environment();

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
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result
        assert!(result.is_err());
        match result {
            Err(PackageInstallerError::DependencyError(_)) => (),
            _ => panic!("Expected DependencyError"),
        }
    }

    #[test]
    fn test_install_package_installation_error() {
        let (fs, runner, reporter, config) = create_test_environment();

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
        runner.error_response("test install", "Installation failed", 1); // Installation fails

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result
        assert!(result.is_ok()); // We return Ok with a failed status inside
        let install_result = result.unwrap();
        assert_eq!(install_result.package_name, "ripgrep");
        match install_result.status {
            InstallationStatus::Failed(_) => (),
            _ => panic!("Expected Failed status"),
        }
    }

    #[test]
    fn test_installation_result_timing() {
        // Test basic timing
        let result = InstallationResult::success("test", Duration::from_secs(5), None);
        assert_eq!(result.total_duration(), Duration::from_secs(5));
        assert_eq!(result.dependency_duration(), Duration::from_secs(0));

        // Test with dependencies
        let dep1 = InstallationResult::success("dep1", Duration::from_secs(3), None);
        let dep2 = InstallationResult::success("dep2", Duration::from_secs(2), None);

        let result = InstallationResult::success("test", Duration::from_secs(5), None)
            .with_dependencies(vec![dep1, dep2]);

        assert_eq!(result.total_duration(), Duration::from_secs(10)); // 5 + 3 + 2
        assert_eq!(result.dependency_duration(), Duration::from_secs(5)); // 3 + 2
    }
}
