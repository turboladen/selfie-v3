// src/services/package_list_service.rs
// Enhanced implementation of the 'selfie package list' command with command availability checking

use console::style;

use crate::{
    adapters::progress::{ProgressManager, ProgressStyleType},
    domain::{config::Config, package::Package},
    ports::command::CommandRunner,
    ports::package_repo::{PackageRepoError, PackageRepository},
    services::command_validator::CommandValidator,
};

/// Result of running the list command
pub enum PackageListResult {
    /// Package listing successful
    Success(String),
    /// Command failed to run
    Error(String),
}

/// Handles the 'package list' command with enhanced command availability checking
pub struct PackageListService<'a, R: CommandRunner, P: PackageRepository> {
    runner: &'a R,
    config: &'a Config,
    progress_manager: &'a ProgressManager,
    verbose: bool,
    use_colors: bool,
    package_repo: &'a P,
}

impl<'a, R: CommandRunner, P: PackageRepository> PackageListService<'a, R, P> {
    /// Create a new list command handler
    pub fn new(
        runner: &'a R,
        config: &'a Config,
        progress_manager: &'a ProgressManager,
        verbose: bool,
        package_repo: &'a P,
    ) -> Self {
        let use_colors = progress_manager.use_colors();
        Self {
            runner,
            config,
            progress_manager,
            verbose,
            use_colors,
            package_repo,
        }
    }

    /// Execute the list command
    pub fn execute(&self) -> PackageListResult {
        // Create progress display
        let progress = self.progress_manager.create_progress_bar(
            "list",
            "Searching for packages...",
            ProgressStyleType::Spinner,
        );

        // Get list of packages
        match self.list_packages() {
            Ok(output) => {
                progress.finish_with_message("Done");
                PackageListResult::Success(output)
            }
            Err(err) => {
                progress.abandon_with_message("Failed");
                PackageListResult::Error(format!("Error: {}", err))
            }
        }
    }

    /// List packages with compatibility information and command availability
    fn list_packages(&self) -> Result<String, PackageRepoError> {
        let packages = self.package_repo.list_packages()?;

        if packages.is_empty() {
            return Ok("No packages found in package directory.".to_string());
        }

        // Create command validator for checking command availability
        let command_validator = CommandValidator::new(self.runner);

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
            } else if self.use_colors {
                style("Not compatible with current environment")
                    .red()
                    .to_string()
            } else {
                "Not compatible with current environment".to_string()
            };

            output.push_str(&format!(
                "  {} ({}) - {}\n",
                package_name, version, compatibility
            ));

            // Check command availability for compatible packages
            if is_compatible && self.verbose {
                if let Some(env_config) = package.environments.get(&self.config.environment) {
                    // Extract base command
                    if let Some(base_cmd) =
                        CommandValidator::<R>::extract_base_command(&env_config.install)
                    {
                        let cmd_available = self.runner.is_command_available(base_cmd);

                        let status = if cmd_available {
                            if self.use_colors {
                                style("    ✓ Install command available").green().to_string()
                            } else {
                                "    ✓ Install command available".to_string()
                            }
                        } else if self.use_colors {
                            style(format!("    ⚠ Install command '{}' not found", base_cmd))
                                .yellow()
                                .to_string()
                        } else {
                            format!("    ⚠ Install command '{}' not found", base_cmd)
                        };

                        output.push_str(&format!("{}\n", status));
                    }

                    // Check for environment-specific recommendations
                    if let Some(recommendation) = command_validator.is_command_recommended_for_env(
                        &self.config.environment,
                        &env_config.install,
                    ) {
                        let recommendation_text = if self.use_colors {
                            style(format!("    ℹ {}", recommendation))
                                .blue()
                                .to_string()
                        } else {
                            format!("    ℹ {}", recommendation)
                        };

                        output.push_str(&format!("{}\n", recommendation_text));
                    }
                }
            }

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

                // Check for potential issues in commands
                if is_compatible {
                    if let Some(env_config) = package.environments.get(&self.config.environment) {
                        let mut warnings = Vec::new();

                        if command_validator.might_require_sudo(&env_config.install) {
                            warnings.push("might require sudo privileges");
                        }

                        if command_validator.uses_backticks(&env_config.install) {
                            warnings.push("uses backticks (consider $() instead)");
                        }

                        if command_validator.might_download_content(&env_config.install) {
                            warnings.push("may download content from internet");
                        }

                        if !warnings.is_empty() {
                            let warning_text = if self.use_colors {
                                style(format!("    ⚠ Command notes: {}", warnings.join(", ")))
                                    .yellow()
                                    .to_string()
                            } else {
                                format!("    ⚠ Command notes: {}", warnings.join(", "))
                            };

                            output.push_str(&format!("{}\n", warning_text));
                        }
                    }
                }

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

    /// Filter packages by various criteria
    pub fn filter_packages(&self, packages: &[Package], filter: Option<&str>) -> Vec<Package> {
        // If no filter, return all packages
        if filter.is_none() {
            return packages.to_vec();
        }

        let filter = filter.unwrap().to_lowercase();

        packages
            .iter()
            .filter(|package| {
                // Match on package name
                if package.name.to_lowercase().contains(&filter) {
                    return true;
                }

                // Match on description
                if let Some(desc) = &package.description {
                    if desc.to_lowercase().contains(&filter) {
                        return true;
                    }
                }

                // Match on environment
                for env_name in package.environments.keys() {
                    if env_name.to_lowercase().contains(&filter) {
                        return true;
                    }
                }

                false
            })
            .cloned()
            .collect()
    }

    /// Group packages by environment compatibility
    pub fn list_packages_by_environment(&self) -> Result<String, PackageRepoError> {
        let packages = self.package_repo.list_packages()?;

        if packages.is_empty() {
            return Ok("No packages found in package directory.".to_string());
        }

        let mut output = String::from("Packages by environment compatibility:\n\n");

        // Identify current environment
        let current_env = &self.config.environment;

        // Compatible with current environment
        let compatible: Vec<_> = packages
            .iter()
            .filter(|pkg| pkg.environments.contains_key(current_env))
            .collect();

        // Not compatible with current environment
        let incompatible: Vec<_> = packages
            .iter()
            .filter(|pkg| !pkg.environments.contains_key(current_env))
            .collect();

        // Format section for compatible packages
        let compatible_heading = if self.use_colors {
            style(format!(
                "Compatible with current environment ({}):",
                current_env
            ))
            .green()
            .bold()
            .to_string()
        } else {
            format!("Compatible with current environment ({}):", current_env)
        };

        output.push_str(&format!("{}\n", compatible_heading));

        if compatible.is_empty() {
            output.push_str("  No packages found\n");
        } else {
            for package in compatible {
                let pkg_text = if self.use_colors {
                    format!(
                        "  {} ({})",
                        style(&package.name).magenta().bold(),
                        style(format!("v{}", &package.version)).dim()
                    )
                } else {
                    format!("  {} (v{})", package.name, package.version)
                };

                output.push_str(&format!("{}\n", pkg_text));
            }
        }

        // Format section for incompatible packages
        output.push('\n');
        let incompatible_heading = if self.use_colors {
            style("Not compatible with current environment:")
                .red()
                .bold()
                .to_string()
        } else {
            "Not compatible with current environment:".to_string()
        };

        output.push_str(&format!("{}\n", incompatible_heading));

        if incompatible.is_empty() {
            output.push_str("  No packages found\n");
        } else {
            for package in incompatible {
                let pkg_text = if self.use_colors {
                    format!(
                        "  {} ({}) - Available for: {}",
                        style(&package.name).magenta(),
                        style(format!("v{}", &package.version)).dim(),
                        package
                            .environments
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                } else {
                    format!(
                        "  {} (v{}) - Available for: {}",
                        package.name,
                        package.version,
                        package
                            .environments
                            .keys()
                            .cloned()
                            .collect::<Vec<_>>()
                            .join(", ")
                    )
                };

                output.push_str(&format!("{}\n", pkg_text));
            }
        }

        Ok(output)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        adapters::package_repo::yaml::YamlPackageRepository,
        domain::{config::ConfigBuilder, package::PackageBuilder},
        ports::{
            command::MockCommandRunner,
            filesystem::{MockFileSystem, MockFileSystemExt},
        },
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
        let repo = YamlPackageRepository::new(&fs, config.expanded_package_directory());
        let runner = MockCommandRunner::new();
        let manager = ProgressManager::new(false, false, false);

        let cmd = PackageListService::new(&runner, &config, &manager, false, &repo);

        let result = cmd.list_packages();
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

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(package_dir, true);

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
        let package1_path = package_dir.join("ripgrep.yaml");
        let package2_path = package_dir.join("fzf.yaml");

        fs.mock_list_directory(package_dir, &[&package1_path, &package2_path]);
        fs.mock_read_file(package1_path, package1_yaml);
        fs.mock_read_file(package2_path, package2_yaml);

        // Create a repository with our mock filesystem
        let repo = YamlPackageRepository::new(&fs, config.expanded_package_directory());

        // Create mock runner that shows 'brew' as available
        let mut runner = MockCommandRunner::new();
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("brew"))
            .returning(|_| true);

        let manager = ProgressManager::new(false, false, false);
        let cmd = PackageListService::new(&runner, &config, &manager, false, &repo);

        // Test the list_packages function with our repo
        let result = cmd.list_packages();
        assert!(result.is_ok());
        let output = result.unwrap();
        dbg!(&output);

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

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(package_dir, true);

        // Add test package with detailed information
        let package_yaml = r#"
            name: ripgrep
            version: 1.0.0
            description: Fast search tool
            homepage: https://github.com/BurntSushi/ripgrep
            environments:
              test-env:
                install: sudo apt install ripgrep
        "#;

        let package_path = package_dir.join("ripgrep.yaml");
        fs.mock_list_directory(package_dir, &[&package_path]);
        fs.mock_read_file(package_path, package_yaml);

        // Create a repository with our mock filesystem
        let repo = YamlPackageRepository::new(&fs, config.expanded_package_directory());

        // Create mock runner with command availability
        let mut runner = MockCommandRunner::new();
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("sudo"))
            .returning(|_| true);
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("apt"))
            .returning(|_| true);

        let manager = ProgressManager::new(false, false, false);
        let cmd = PackageListService::new(&runner, &config, &manager, true, &repo);

        // Test the list_packages function with our repo
        let result = cmd.list_packages();
        assert!(result.is_ok());
        let output = result.unwrap();

        // Check verbose information
        assert!(output.contains("Description: Fast search tool"));
        assert!(output.contains("Homepage: https://github.com/BurntSushi/ripgrep"));
        assert!(output.contains("Environments: test-env"));
        assert!(output.contains("Path: /test/packages/ripgrep.yaml"));

        // Check command warnings
        assert!(output.contains("Command notes:"));
        assert!(output.contains("sudo privileges"));
    }

    #[test]
    fn test_filter_packages() {
        // Create some test packages
        let package1 = PackageBuilder::default()
            .name("ripgrep")
            .version("1.0.0")
            .description("Fast search tool")
            .environment("test-env", "install cmd")
            .build();

        let package2 = PackageBuilder::default()
            .name("fzf")
            .version("1.0.0")
            .description("Fuzzy finder")
            .environment("mac-env", "install cmd")
            .build();

        let package3 = PackageBuilder::default()
            .name("cargo-binstall")
            .version("1.0.0")
            .description("Cargo binary installer")
            .environment("linux-env", "install cmd")
            .build();

        let packages = vec![package1, package2, package3];

        // Create the service
        let fs = MockFileSystem::default();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();
        let repo = YamlPackageRepository::new(&fs, config.expanded_package_directory());
        let runner = MockCommandRunner::new();
        let manager = ProgressManager::new(false, false, false);
        let cmd = PackageListService::new(&runner, &config, &manager, false, &repo);

        // Filter by name
        let filtered = cmd.filter_packages(&packages, Some("rip"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "ripgrep");

        // Filter by description
        let filtered = cmd.filter_packages(&packages, Some("fuzzy"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "fzf");

        // Filter by environment
        let filtered = cmd.filter_packages(&packages, Some("linux"));
        assert_eq!(filtered.len(), 1);
        assert_eq!(filtered[0].name, "cargo-binstall");

        // No filter should return all
        let filtered = cmd.filter_packages(&packages, None);
        assert_eq!(filtered.len(), 3);
    }

    #[test]
    fn test_list_packages_by_environment() {
        let mut fs = MockFileSystem::default();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(package_dir, true);

        // Add test package files with different environments
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

        let package3_yaml = r#"
            name: typos-cli
            version: 1.0.0
            environments:
              test-env:
                install: cargo install typos-cli
              other-env:
                install: brew install typos-cli
        "#;

        let package1_path = package_dir.join("ripgrep.yaml");
        let package2_path = package_dir.join("fzf.yaml");
        let package3_path = package_dir.join("typos-cli.yaml");

        fs.mock_list_directory(
            package_dir,
            &[&package1_path, &package2_path, &package3_path],
        );
        fs.mock_read_file(package1_path, package1_yaml);
        fs.mock_read_file(package2_path, package2_yaml);
        fs.mock_read_file(package3_path, package3_yaml);

        // Create a repository with our mock filesystem
        let repo = YamlPackageRepository::new(&fs, config.expanded_package_directory());
        let runner = MockCommandRunner::new();
        let manager = ProgressManager::new(false, false, false);
        let cmd = PackageListService::new(&runner, &config, &manager, false, &repo);

        // Test grouping by environment
        let result = cmd.list_packages_by_environment();
        assert!(result.is_ok());
        let output = result.unwrap();

        // Check that packages are correctly grouped
        assert!(output.contains("Compatible with current environment"));
        assert!(output.contains("Not compatible with current environment"));

        // Compatible packages should include ripgrep and typos-cli
        let compat_section = output.split("Not compatible").next().unwrap();
        assert!(compat_section.contains("ripgrep"));
        assert!(compat_section.contains("typos-cli"));

        // Incompatible section should include fzf
        let incompat_section = output.split("Not compatible").nth(1).unwrap();
        assert!(incompat_section.contains("fzf"));

        // fzf should show its available environments
        assert!(incompat_section.contains("Available for: other-env"));
    }
}
