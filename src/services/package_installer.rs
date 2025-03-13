// src/services/package_installer.rs
mod dependency;

use std::time::Instant;

use console::style;
use dependency::{DependencyResolver, DependencyResolverError};
use thiserror::Error;

use crate::{
    adapters::{package_repo::yaml::YamlPackageRepository, progress::ProgressManager},
    domain::{
        config::AppConfig,
        errors::{
            EnhancedCommandError, EnhancedDependencyError, EnhancedPackageError, ErrorContext,
        },
        installation::{Installation, InstallationError, InstallationResult, InstallationStatus},
        package::Package,
    },
    ports::{
        command::{CommandError, CommandRunner},
        filesystem::{FileSystem, FileSystemError},
        package_repo::{PackageRepoError, PackageRepository},
    },
    services::enhanced_error_handler::EnhancedErrorHandler,
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
    fs: &'a dyn FileSystem,
    runner: &'a dyn CommandRunner,
    config: &'a AppConfig,
    progress_manager: &'a ProgressManager,
    check_commands: bool,
}

impl<'a> PackageInstaller<'a> {
    pub(crate) fn new(
        fs: &'a dyn FileSystem,
        runner: &'a dyn CommandRunner,
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
    pub(crate) async fn install_package(
        &self,
        package_name: &str,
    ) -> Result<InstallationResult, PackageInstallerError> {
        // Create package repository
        let package_repo = YamlPackageRepository::new(
            self.fs,
            self.config.expanded_package_directory(),
            self.progress_manager,
        );

        // Create error handler if needed
        let error_handler =
            EnhancedErrorHandler::new(self.fs, &package_repo, self.progress_manager);

        // Start timing the entire process
        let start_time = Instant::now();

        // First, find the package to get its details
        let main_package = package_repo
            .get_package(package_name)
            .map_err(|e| match e {
                PackageRepoError::PackageNotFound(name) => {
                    // Use enhanced error handling for not found errors
                    let error_msg = error_handler.handle_package_not_found(&name);
                    PackageInstallerError::PackageNotFound(error_msg)
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

        self.progress_manager.print_info(header);

        // ╭──────────────────────╮
        // │ Resolve dependencies │
        // ╰──────────────────────╯
        let packages = match self.resolve_dependencies(package_name, &package_repo) {
            Ok(packages) => packages,
            Err(err) => {
                // Use enhanced error handling for dependency errors
                if let PackageInstallerError::CircularDependency(cycle_str) = &err {
                    if let Some(cycle) = self.parse_cycle_string(cycle_str) {
                        let error_msg = error_handler.handle_circular_dependency(&cycle);
                        self.progress_manager.print_error(&error_msg);
                        return Err(PackageInstallerError::EnhancedError(error_msg));
                    }
                }

                self.progress_manager
                    .print_error(format!("Dependency resolution failed: {}", err));
                return Err(err);
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
                self.progress_manager.print_info(dependency_header);

                // Make sure the dep has info for this environment
                if !package.environments.contains_key(self.config.environment()) {
                    dependency_results.push(InstallationResult {
                        package_name: package.name.clone(),
                        status: InstallationStatus::Skipped(format!(
                            "Package `{}` does not support current environment (`{}` section)",
                            &package_name,
                            self.config.environment()
                        )),
                        duration: start_time.elapsed(),
                        dependencies: vec![],
                        command_output: None,
                    });

                    continue;
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

    /// Check if a package can be installed in the current environment
    pub(crate) async fn check_package_installable(
        &self,
        package_name: &str,
    ) -> Result<bool, PackageInstallerError> {
        // Create a package repository
        let package_repo = YamlPackageRepository::new(
            self.fs,
            self.config.expanded_package_directory(),
            self.progress_manager,
        );

        // Create error handler if needed
        let error_handler =
            EnhancedErrorHandler::new(self.fs, &package_repo, self.progress_manager);

        // Find the package
        let package = package_repo
            .get_package(package_name)
            .map_err(|e| match e {
                PackageRepoError::PackageNotFound(name) => {
                    // Use enhanced error handling for not found errors
                    let error_msg = error_handler.handle_package_not_found(&name);
                    PackageInstallerError::EnhancedError(error_msg)
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
                    if !self.runner.is_command_available(base_cmd).await {
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
    async fn verify_commands(&self, packages: &[Package]) -> Result<bool, PackageInstallerError> {
        // Check commands for each package
        let mut missing_commands = Vec::new();

        for package in packages {
            if let Some(env_config) = package.environments.get(self.config.environment()) {
                // Extract and check base command
                if let Some(base_cmd) = Self::extract_base_command(&env_config.install) {
                    if !self.runner.is_command_available(base_cmd).await {
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
    ) -> Result<InstallationResult, PackageInstallerError> {
        let start_time = Instant::now();
        let indent = " ".repeat(indent_level);

        // Resolve environment configuration with enhanced error context
        let env_config = self.config.resolve_environment(package).map_err(|e| {
            // Create an enhanced error with context
            let context = ErrorContext::default()
                .with_package(&package.name)
                .with_environment(self.config.environment())
                .with_message(&format!("Original error: {}", e)); // Add the original error message

            let enhanced_error = EnhancedPackageError::environment_not_supported(
                self.config.environment(),
                &package.name,
            )
            .with_context(context);

            PackageInstallerError::from(enhanced_error)
        })?;

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
            .await
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
            self.progress_manager.print_success(status_message);

            return Ok(InstallationResult::already_installed(
                &package.name,
                check_duration,
            ));
        } else {
            // Print "Not installed" with duration
            self.progress_manager
                .print_progress(self.progress_manager.with_duration(
                    format!("{}✓ Checking installation status: Not installed", indent),
                    Some(check_duration),
                ));
        }

        // Print installing message
        self.progress_manager
            .print_progress(format!("{}⌛ Installing...", indent));

        // Execute installation with enhanced error handling
        let result = match installation.execute_install(self.runner).await {
            Ok(output) => {
                let duration = start_time.elapsed();
                installation.complete(InstallationStatus::Complete);

                // Print completion message with duration
                let complete_message =
                    format!("{}✓ Installation complete ({:.1?})", indent, duration);
                self.progress_manager.print_success(complete_message);

                InstallationResult::success(&package.name, duration, Some(output))
            }
            Err(err) => {
                let duration = start_time.elapsed();

                installation.update_status(InstallationStatus::Failed(format!(
                    "Installation error: {}",
                    err
                )));

                // Use enhanced error handler for command errors if available
                if self.progress_manager.verbose()
                    && matches!(err, InstallationError::CommandError(_))
                {
                    // Get install command string
                    let cmd = &env_config.install;

                    // Create a package repository
                    let package_repo = YamlPackageRepository::new(
                        self.fs,
                        self.config.expanded_package_directory(),
                        self.progress_manager,
                    );
                    let error_handler =
                        EnhancedErrorHandler::new(self.fs, &package_repo, self.progress_manager);

                    // Extract command failure details if possible
                    if let InstallationError::CommandError(CommandError::ExecutionError(e)) = &err {
                        // Use the error handler to format the command error
                        // This will provide better formatted details about the command failure
                        let error_message = error_handler.handle_command_error(
                            cmd, // Use the command string
                            1,   // Default exit code
                            "",  // No stdout available
                            e,   // Error message as stderr
                        );

                        self.progress_manager.print_verbose(&error_message);
                        return Err(PackageInstallerError::EnhancedError(error_message));
                    }
                }

                // Print error message
                let error_message = format!(
                    "{}✗ Installation failed: {} ({:.1?})",
                    indent, err, duration
                );
                self.progress_manager.print_error(error_message);

                return Err(PackageInstallerError::InstallationError(err));
            }
        };

        if self.progress_manager.verbose() && result.command_output.is_some() {
            let output = result.command_output.as_ref().unwrap();
            self.progress_manager.print_verbose("Command stdout:");
            for line in output.stdout.lines() {
                self.progress_manager.print_verbose(format!("  {}", line));
            }

            if !output.stderr.is_empty() {
                self.progress_manager.print_verbose("Command stderr:");
                for line in output.stderr.lines() {
                    self.progress_manager.print_verbose(format!("  {}", line));
                }
            }
        }

        Ok(result)
    }

    /// Report the final installation status with timing information
    fn report_final_status(&self, result: &InstallationResult) {
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
        command.split_whitespace().next()
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
    use std::path::Path;

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
            ProgressManager::new(false, true),
        )
    }

    #[tokio::test]
    async fn test_installation_manager_install_success() {
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
        let result = manager.install_package(&package.name).await;

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::Complete);
    }

    #[tokio::test]
    async fn test_installation_manager_already_installed() {
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
        let result = manager.install_package(&package.name).await;

        assert!(result.is_ok());
        let installation = result.unwrap();
        assert_eq!(installation.status, InstallationStatus::AlreadyInstalled);
    }

    #[tokio::test]
    async fn test_installation_manager_install_failure() {
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
        let result = manager.install_package(&package.name).await;

        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_installation_manager_environment_incompatible() {
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
        let result = manager.install_package(&package.name).await;

        assert!(result.is_err());
    }

    #[test]
    fn test_parse_cycle_string() {
        let config = create_test_config();
        let (fs, runner, progress_manager) = create_installer_deps();
        let manager = PackageInstaller::new(&fs, &runner, &config, &progress_manager, true);

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
        let mut fs = MockFileSystem::default();
        let mut runner = MockCommandRunner::new();

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

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(&package_dir, true);

        let ripgrep = package_dir.join("ripgrep.yaml");
        fs.mock_path_exists(&ripgrep, true);
        fs.mock_path_exists(package_dir.join("ripgrep.yml"), false);
        fs.mock_read_file(&ripgrep, package_yaml);

        // Set up mock command responses
        runner.error_response("rg check", "Not found", 1); // Not installed
        runner.success_response("rg install", "Installed successfully");

        let progress_manager = ProgressManager::new(false, true);

        // Create package installer (using the new consolidated version)
        let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, false);

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
        let mut fs = MockFileSystem::default();
        let mut runner = MockCommandRunner::new();

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

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(&package_dir, true);

        let ripgrep = package_dir.join("ripgrep.yaml");
        fs.mock_path_exists(&ripgrep, true);
        fs.mock_path_exists(package_dir.join("ripgrep.yml"), false);
        fs.mock_read_file(&ripgrep, package_yaml);

        let rust = package_dir.join("rust.yaml");
        fs.mock_path_exists(&rust, true);
        fs.mock_path_exists(package_dir.join("rust.yml"), false);
        fs.mock_read_file(&rust, dependency_yaml);

        // Set up mock command responses
        runner.error_response("rg check", "Not found", 1); // Not installed
        runner.success_response("rg install", "Installed successfully");
        runner.error_response("rust check", "Not found", 1); // Not installed
        runner.success_response("rust install", "Installed successfully");

        let progress_manager = ProgressManager::new(false, true);

        // Create package installer
        let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, false);

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
        let mut fs = MockFileSystem::default();
        let mut runner = MockCommandRunner::new();

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

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(&package_dir, true);

        let main_pkg = package_dir.join("main-pkg.yaml");
        fs.mock_path_exists(&main_pkg, true);
        fs.mock_path_exists(package_dir.join("main-pkg.yml"), false);
        fs.mock_read_file(&main_pkg, main_pkg_yaml);

        let dep1 = package_dir.join("dep1.yaml");
        fs.mock_path_exists(&dep1, true);
        fs.mock_path_exists(package_dir.join("dep1.yml"), false);
        fs.mock_read_file(&dep1, dep1_yaml);

        let dep2 = package_dir.join("dep2.yaml");
        fs.mock_path_exists(&dep2, true);
        fs.mock_path_exists(package_dir.join("dep2.yml"), false);
        fs.mock_read_file(&dep2, dep2_yaml);

        let dep3 = package_dir.join("dep3.yaml");
        fs.mock_path_exists(&dep3, true);
        fs.mock_path_exists(package_dir.join("dep3.yml"), false);
        fs.mock_read_file(&dep3, dep3_yaml);

        // Set up mock command responses - all need to be installed
        runner.error_response("main-check", "Not found", 1);
        runner.success_response("main-install", "Installed successfully");
        runner.error_response("dep1-check", "Not found", 1);
        runner.success_response("dep1-install", "Installed successfully");
        runner.error_response("dep2-check", "Not found", 1);
        runner.success_response("dep2-install", "Installed successfully");
        runner.error_response("dep3-check", "Not found", 1);
        runner.success_response("dep3-install", "Installed successfully");

        let progress_manager = ProgressManager::new(false, true);

        // Create package installer
        let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, false);

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
