// src/package_installer.rs
mod dependency;

use std::time::{Duration, Instant};

use thiserror::Error;

use crate::{
    command::{CommandError, CommandOutput, CommandRunner},
    config::Config,
    filesystem::{FileSystem, FileSystemError},
    installation::{InstallationError, InstallationManager, InstallationStatus},
    package::PackageNode,
    package_repo::PackageRepoError,
    progress::ProgressReporter,
};

use dependency::{DependencyResolver, DependencyResolverError};

#[derive(Error, Debug)]
pub enum PackageInstallerError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("FileSystem error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("Package repository error: {0}")]
    PackageRepoError(#[from] PackageRepoError),

    #[error("Dependency error: {0}")]
    DependencyError(#[from] DependencyResolverError),

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

        // Create dependency resolver
        let resolver = DependencyResolver::new(&self.fs, &self.config);

        // Resolve dependencies
        println!("{}", self.reporter.info("Resolving dependencies..."));

        // Start timing the entire process
        let start_time = Instant::now();

        let packages = resolver.resolve_dependencies(package_name)?;

        if packages.len() > 1 {
            println!(
                "{}",
                self.reporter.info(&format!(
                    "Found {} packages to install (including dependencies)",
                    packages.len()
                ))
            );
        }

        // Get the main package (last in the list)
        let main_package = packages.last().unwrap().clone();

        // Install all packages in order
        let mut dependency_results = Vec::new();

        // All packages except the last one are dependencies
        for package in packages.iter().take(packages.len() - 1) {
            println!(
                "{}",
                self.reporter
                    .info(&format!("Installing dependency '{}'", package.name))
            );

            // Install dependency and stop immediately on failure
            match self.install_single_package(package) {
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
                    // Immediately return any error during dependency installation
                    return Err(err);
                }
            }
        }

        // Only install the main package if all dependencies were successfully installed
        println!(
            "{}",
            self.reporter
                .info(&format!("Installing main package '{}'", main_package.name))
        );
        let main_result = self.install_single_package(&main_package)?;

        // Get the total installation time
        let total_duration = start_time.elapsed();

        // Create the final result with dependencies
        let mut final_result = main_result.with_dependencies(dependency_results);

        // Override the duration with the total time
        println!("Overriding duration: {:?}", total_duration);
        final_result.duration = total_duration;

        // Report final status including timing information
        self.report_final_status(&final_result);

        Ok(final_result)
    }

    /// Install a single package (no dependency handling)
    fn install_single_package(
        &self,
        package: &PackageNode,
    ) -> Result<InstallationResult, PackageInstallerError> {
        let start_time = Instant::now();

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

        Ok(result)
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

    use dependency::DependencyResolverError;

    use super::*;

    use crate::{
        command::mock::MockCommandRunner, config::ConfigBuilder, filesystem::mock::MockFileSystem,
        progress::ConsoleRenderer,
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

    #[test]
    fn test_install_package_success() {
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
            Err(PackageInstallerError::DependencyError(
                DependencyResolverError::PackageNotFound(name),
            )) => {
                assert_eq!(name, "nonexistent");
            }
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

        // Make sure we don't get to the main package install by NOT providing a response for it
        // This will cause an error if the code tries to install the main package after a dependency fails
        // We don't need to add responses for the main package's commands

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("ripgrep");

        // Verify the result - we explicitly expect an error of type InstallationError
        assert!(result.is_err());
        match result {
            Err(PackageInstallerError::InstallationError(_)) => (), // This is what we expect
            other => panic!("Expected InstallationError, got {:?}", other),
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

    #[test]
    fn test_install_package_with_complex_dependencies() {
        let (fs, runner, reporter, config) = create_test_environment();

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
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("main-pkg");

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();

        // Main package was installed correctly
        assert_eq!(install_result.package_name, "main-pkg");
        assert_eq!(install_result.status, InstallationStatus::Complete);

        // Dependencies were installed
        assert_eq!(install_result.dependencies.len(), 2);

        // dep2 should be first in the dependency list since it's deepest
        assert_eq!(install_result.dependencies[0].package_name, "dep2");
        assert_eq!(
            install_result.dependencies[0].status,
            InstallationStatus::Complete
        );

        // dep1 should be second
        assert_eq!(install_result.dependencies[1].package_name, "dep1");
        assert_eq!(
            install_result.dependencies[1].status,
            InstallationStatus::Complete
        );
    }

    #[test]
    fn test_install_package_with_dependency_already_installed() {
        let (fs, runner, reporter, config) = create_test_environment();

        // Set up package files with one dependency
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
    "#;

        fs.add_file(
            std::path::Path::new("/test/packages/main-pkg.yaml"),
            main_pkg_yaml,
        );
        fs.add_file(std::path::Path::new("/test/packages/dep1.yaml"), dep1_yaml);
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Set up mock command responses
        runner.error_response("main-check", "Not found", 1);
        runner.success_response("main-install", "Installed successfully");
        runner.success_response("dep1-check", "Already installed"); // Dependency already installed

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("main-pkg");

        // Verify the result
        assert!(result.is_ok());
        let install_result = result.unwrap();

        // Main package was installed correctly
        assert_eq!(install_result.package_name, "main-pkg");
        assert_eq!(install_result.status, InstallationStatus::Complete);

        // Dependency was already installed
        assert_eq!(install_result.dependencies.len(), 1);
        assert_eq!(install_result.dependencies[0].package_name, "dep1");
        assert_eq!(
            install_result.dependencies[0].status,
            InstallationStatus::AlreadyInstalled
        );
    }

    #[test]
    fn test_install_package_with_missing_dependency() {
        let (fs, runner, reporter, config) = create_test_environment();

        // Set up package file with a non-existent dependency
        let main_pkg_yaml = r#"
        name: main-pkg
        version: 1.0.0
        environments:
          test-env:
            install: main-install
            check: main-check
            dependencies:
              - missing-dep
    "#;

        fs.add_file(
            std::path::Path::new("/test/packages/main-pkg.yaml"),
            main_pkg_yaml,
        );
        fs.add_existing_path(std::path::Path::new("/test/packages"));

        // Create the installer
        let installer = PackageInstaller::new(fs, runner, config, reporter, false);

        // Run the installation
        let result = installer.install_package("main-pkg");

        // Verify the result
        assert!(result.is_err());
        match result {
            Err(PackageInstallerError::DependencyError(
                DependencyResolverError::PackageNotFound(name),
            )) => {
                assert_eq!(name, "missing-dep");
            }
            _ => panic!("Expected package not found error"),
        }
    }
}
