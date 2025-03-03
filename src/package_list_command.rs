// src/package_list_command.rs
// Implements the 'selfie package list' command

use console::style;

use crate::{
    command::CommandRunner,
    config::Config,
    package_repo::{PackageRepoError, PackageRepository},
    ports::filesystem::FileSystem,
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
    use_colors: bool, // Added this field
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
        // Get the color setting from progress_manager
        let use_colors = progress_manager.use_colors();
        Self {
            fs,
            _runner: runner,
            config,
            progress_manager,
            verbose,
            use_colors,
        }
    }

    /// Execute the list command
    pub fn execute(&self) -> ListCommandResult {
        // Create progress display - use a generic message
        let progress = self.progress_manager.create_progress_bar(
            "list",
            "Searching for packages...",
            ProgressStyleType::Spinner,
        );

        // Create package repository
        let package_repo =
            PackageRepository::new(self.fs, self.config.expanded_package_directory());

        // Get list of packages
        match self.list_packages(&package_repo) {
            Ok(output) => {
                // Just show a simple success message in the progress bar
                progress.finish_with_message("Done");
                ListCommandResult::Success(output)
            }
            Err(err) => {
                // Simplify the progress bar message
                progress.abandon_with_message("Failed");
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
            let is_compatible = package.environments.contains_key(&self.config.environment);

            // Style the package name and version with color
            let package_name = if self.use_colors {
                style(&package.name).magenta().bold().to_string()
            } else {
                package.name.clone()
            };

            let version = if self.use_colors {
                style(format!("v{}", &package.version)).dim().to_string()
            } else {
                format!("v{}", &package.version)
            };

            // Style the compatibility message with color
            let compatibility = if is_compatible {
                if self.use_colors {
                    style("Compatible with current environment")
                        .green()
                        .to_string()
                } else {
                    "Compatible with current environment".to_string()
                }
            } else {
                if self.use_colors {
                    style("Not compatible with current environment")
                        .red()
                        .to_string()
                } else {
                    "Not compatible with current environment".to_string()
                }
            };

            output.push_str(&format!(
                "  {} ({}) - {}\n",
                package_name, version, compatibility
            ));

            // Add more details if verbose mode is enabled
            if self.verbose {
                if let Some(desc) = &package.description {
                    let description = if self.use_colors {
                        style(format!("    Description: {}", desc))
                            .blue()
                            .to_string()
                    } else {
                        format!("    Description: {}", desc)
                    };
                    output.push_str(&format!("{}\n", description));
                }

                if let Some(homepage) = &package.homepage {
                    let homepage_text = if self.use_colors {
                        style(format!("    Homepage: {}", homepage))
                            .blue()
                            .to_string()
                    } else {
                        format!("    Homepage: {}", homepage)
                    };
                    output.push_str(&format!("{}\n", homepage_text));
                }

                // Show environments
                output.push_str("    Environments: ");
                let env_list: Vec<String> = package.environments.keys().cloned().collect();
                output.push_str(&env_list.join(", "));
                output.push('\n');

                // Show file path if available
                if let Some(path) = &package.path {
                    let path_text = if self.use_colors {
                        style(format!("    Path: {}", path.display()))
                            .dim()
                            .to_string()
                    } else {
                        format!("    Path: {}", path.display())
                    };
                    output.push_str(&format!("{}\n", path_text));
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
        command::MockCommandRunner,
        config::ConfigBuilder,
        ports::filesystem::{MockFileSystem, MockFileSystemExt},
    };
    use std::path::Path;

    #[test]
    fn test_list_empty_directory() {
        let mut fs = MockFileSystem::default();
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
        let mut fs = MockFileSystem::default();
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
        let mut fs = MockFileSystem::default();
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
