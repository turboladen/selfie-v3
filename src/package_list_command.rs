// src/package_list_command.rs
// Implements the 'selfie package list' command

use crate::{
    command::CommandRunner,
    config::Config,
    filesystem::FileSystem,
    package_repo::{PackageRepoError, PackageRepository},
    progress_display::{ProgressManager, ProgressStyleType},
};

/// Result of running the list command
pub enum ListCommandResult {
    /// Package listing successful
    Success(String),
    /// Command failed to run
    Error(String),
}

/// Handles the 'package list' command
pub struct ListCommand<'a, F: FileSystem, R: CommandRunner> {
    fs: &'a F,
    _runner: &'a R,
    config: Config,
    progress_manager: &'a ProgressManager,
    verbose: bool,
}

impl<'a, F: FileSystem, R: CommandRunner> ListCommand<'a, F, R> {
    /// Create a new list command handler
    pub fn new(
        fs: &'a F,
        runner: &'a R,
        config: Config,
        progress_manager: &'a ProgressManager,
        verbose: bool,
    ) -> Self {
        Self {
            fs,
            _runner: runner,
            config,
            progress_manager,
            verbose,
        }
    }

    /// Execute the list command
    pub fn execute(&self) -> ListCommandResult {
        // Create progress display
        let progress = self.progress_manager.create_progress_bar(
            "list",
            "Listing packages...",
            ProgressStyleType::Spinner,
        );

        // Create package repository
        let package_repo =
            PackageRepository::new(self.fs, self.config.expanded_package_directory());

        // Get list of packages
        match self.list_packages(&package_repo) {
            Ok(output) => {
                progress.finish_with_message("Package listing complete");
                ListCommandResult::Success(output)
            }
            Err(err) => {
                progress.abandon_with_message(format!("Failed to list packages: {}", err));
                ListCommandResult::Error(format!("Error: {}", err))
            }
        }
    }

    /// List packages with compatibility information
    fn list_packages<FS: FileSystem>(
        &self,
        repo: &PackageRepository<FS>,
    ) -> Result<String, PackageRepoError> {
        let packages = repo.list_packages()?;

        if packages.is_empty() {
            return Ok("No packages found in package directory.".to_string());
        }

        let mut output = String::from("Available packages:\n");

        // Sort packages by name for consistent output
        let mut sorted_packages = packages;
        sorted_packages.sort_by(|a, b| a.name.cmp(&b.name));

        for package in sorted_packages {
            let compatibility = if package.environments.contains_key(&self.config.environment) {
                "Compatible with current environment"
            } else {
                "Not compatible with current environment"
            };

            output.push_str(&format!(
                "  {} (v{}) - {}\n",
                package.name, package.version, compatibility
            ));

            // Add more details if verbose mode is enabled
            if self.verbose {
                if let Some(desc) = &package.description {
                    output.push_str(&format!("    Description: {}\n", desc));
                }

                if let Some(homepage) = &package.homepage {
                    output.push_str(&format!("    Homepage: {}\n", homepage));
                }

                // Show environments
                output.push_str("    Environments: ");
                let env_list: Vec<String> = package.environments.keys().cloned().collect();
                output.push_str(&env_list.join(", "));
                output.push('\n');

                // Show file path if available
                if let Some(path) = &package.path {
                    output.push_str(&format!("    Path: {}\n", path.display()));
                }

                // Add a separator line between packages
                output.push('\n');
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        command::mock::MockCommandRunner, config::ConfigBuilder, filesystem::mock::MockFileSystem,
    };
    use std::path::Path;

    #[test]
    fn test_list_empty_directory() {
        let fs = MockFileSystem::default();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        fs.add_existing_path(Path::new("/test/packages"));

        // Create a repository with an empty directory
        let repo = PackageRepository::new(&fs, config.expanded_package_directory());
        let runner = MockCommandRunner::new();
        let manager = ProgressManager::new(false, false, false);

        let cmd = ListCommand::new(&fs, &runner, config, &manager, false);

        let result = cmd.list_packages(&repo);
        assert!(result.is_ok());
        assert!(result.unwrap().contains("No packages found"));
    }

    #[test]
    fn test_list_packages() {
        let fs = MockFileSystem::default();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        fs.add_existing_path(Path::new("/test/packages"));

        // Add test package files to the mock filesystem
        let package1_yaml = r#"
            name: ripgrep
            version: 1.0.0
            environments:
              test-env:
                install: brew install ripgrep
        "#;

        let package2_yaml = r#"
            name: fzf
            version: 0.1.0
            environments:
              other-env:
                install: brew install fzf
        "#;

        fs.add_file(Path::new("/test/packages/ripgrep.yaml"), package1_yaml);
        fs.add_file(Path::new("/test/packages/fzf.yaml"), package2_yaml);

        // Create a real repository with our mock filesystem
        let repo = PackageRepository::new(&fs, config.expanded_package_directory());

        let runner = MockCommandRunner::new();
        let manager = ProgressManager::new(false, false, false);
        let cmd = ListCommand::new(&fs, &runner, config, &manager, false);

        // Test the list_packages function with our repo
        let result = cmd.list_packages(&repo);
        assert!(result.is_ok());
        let output = result.unwrap();

        // Check that both packages are listed
        assert!(output.contains("ripgrep (v1.0.0)"));
        assert!(output.contains("fzf (v0.1.0)"));

        // Check compatibility information
        assert!(output.contains("Compatible with current environment"));
        assert!(output.contains("Not compatible with current environment"));
    }

    #[test]
    fn test_list_packages_verbose() {
        let fs = MockFileSystem::default();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        fs.add_existing_path(Path::new("/test/packages"));

        // Add test package with detailed information
        let package_yaml = r#"
            name: ripgrep
            version: 1.0.0
            description: Fast search tool
            homepage: https://github.com/BurntSushi/ripgrep
            environments:
              test-env:
                install: brew install ripgrep
        "#;

        fs.add_file(Path::new("/test/packages/ripgrep.yaml"), package_yaml);

        // Create a repository with our mock filesystem
        let repo = PackageRepository::new(&fs, config.expanded_package_directory());

        let runner = MockCommandRunner::new();
        let manager = ProgressManager::new(false, false, false);
        let cmd = ListCommand::new(
            &fs, &runner, config, &manager, true, // Verbose mode
        );

        // Test the list_packages function with our repo
        let result = cmd.list_packages(&repo);
        assert!(result.is_ok());
        let output = result.unwrap();

        // Check verbose information
        assert!(output.contains("Description: Fast search tool"));
        assert!(output.contains("Homepage: https://github.com/BurntSushi/ripgrep"));
        assert!(output.contains("Environments: test-env"));
        assert!(output.contains("Path: /test/packages/ripgrep.yaml"));
    }
}
