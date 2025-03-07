// src/services/error_formatter.rs
// Provides consistent formatting for error messages with context and suggestions

use console::style;
use std::error::Error;
use std::path::Path;

use crate::adapters::progress::{MessageType, ProgressManager};
use crate::domain::validation::ValidationResult;

/// Helper for formatting user-friendly error messages
pub struct ErrorFormatter<'a> {
    pub progress_manager: &'a ProgressManager,
}

impl<'a> ErrorFormatter<'a> {
    /// Create a new error formatter
    pub fn new(progress_manager: &'a ProgressManager) -> Self {
        Self { progress_manager }
    }

    /// Format a package not found error with suggestions
    pub fn format_package_not_found(&self, name: &str, suggestions: &[String]) -> String {
        let mut output = String::new();

        let error_header = self.format_header("Package not found", MessageType::Error);
        output.push_str(&error_header);

        let package_name = if self.progress_manager.use_colors() {
            style(name).magenta().bold().to_string()
        } else {
            name.to_string()
        };

        output.push_str(&format!("Cannot find package: {}\n\n", package_name));

        // Add suggestions if any
        if !suggestions.is_empty() {
            output.push_str("Did you mean:\n");

            for suggestion in suggestions {
                let bullet = if self.progress_manager.use_colors() {
                    style("•").cyan().to_string()
                } else {
                    "•".to_string()
                };

                let suggestion_text = if self.progress_manager.use_colors() {
                    style(suggestion).cyan().to_string()
                } else {
                    suggestion.to_string()
                };

                output.push_str(&format!("  {} {}\n", bullet, suggestion_text));
            }

            output.push_str("\n");
        }

        output.push_str("You can use 'selfie package list' to see all available packages.\n");

        output
    }

    /// Format a configuration error
    pub fn format_config_error(&self, error: &dyn Error) -> String {
        let mut output = String::new();

        let error_header = self.format_header("Configuration error", MessageType::Error);
        output.push_str(&error_header);

        // Add the error message
        output.push_str(&format!("{}\n\n", error));

        // Add configuration information
        output.push_str("Please check your configuration:\n");
        output.push_str("  1. Config file location: ~/.config/selfie/config.yaml\n");
        output.push_str("  2. Required fields: environment, package_directory\n");
        output.push_str("  3. Command line overrides: --environment, --package-directory\n");

        output
    }

    /// Format a command execution error
    pub fn format_command_error(
        &self,
        command: &str,
        exit_code: i32,
        stdout: &str,
        stderr: &str,
    ) -> String {
        let mut output = String::new();

        let error_header = self.format_header("Command execution failed", MessageType::Error);
        output.push_str(&error_header);

        // Add command information
        let command_text = if self.progress_manager.use_colors() {
            style(command).cyan().italic().to_string()
        } else {
            format!("'{}'", command)
        };

        output.push_str(&format!("Command: {}\n", command_text));
        output.push_str(&format!("Exit code: {}\n\n", exit_code));

        // Add stdout if present
        if !stdout.trim().is_empty() {
            let stdout_header = if self.progress_manager.use_colors() {
                style("Standard output:").dim().to_string()
            } else {
                "Standard output:".to_string()
            };

            output.push_str(&format!("{}\n", stdout_header));
            for line in stdout.lines() {
                output.push_str(&format!("  {}\n", line));
            }
            output.push_str("\n");
        }

        // Add stderr if present
        if !stderr.trim().is_empty() {
            let stderr_header = if self.progress_manager.use_colors() {
                style("Error output:").red().to_string()
            } else {
                "Error output:".to_string()
            };

            output.push_str(&format!("{}\n", stderr_header));
            for line in stderr.lines() {
                output.push_str(&format!("  {}\n", line));
            }
            output.push_str("\n");
        }

        // Add a note about exit codes
        output.push_str("Note: Only exit code 0 indicates successful execution.\n");
        output.push_str("      Check the command's error output for details.\n");

        output
    }

    /// Format a dependency error
    pub fn format_dependency_error(&self, error: &dyn Error, dependencies: &[String]) -> String {
        let mut output = String::new();

        let error_header = self.format_header("Dependency error", MessageType::Error);
        output.push_str(&error_header);

        // Add the error message
        output.push_str(&format!("{}\n\n", error));

        // Add dependency information
        if !dependencies.is_empty() {
            output.push_str("Dependencies:\n");

            for dependency in dependencies {
                let bullet = if self.progress_manager.use_colors() {
                    style("•").cyan().to_string()
                } else {
                    "•".to_string()
                };

                output.push_str(&format!("  {} {}\n", bullet, dependency));
            }
            output.push_str("\n");
        }

        output.push_str("Please resolve the dependency issues before trying again.\n");

        output
    }

    /// Format a permission error
    pub fn format_permission_error(&self, path: &Path, action: &str) -> String {
        let mut output = String::new();

        let error_header = self.format_header("Permission denied", MessageType::Error);
        output.push_str(&error_header);

        let path_text = if self.progress_manager.use_colors() {
            style(path.display().to_string()).cyan().to_string()
        } else {
            path.display().to_string()
        };

        output.push_str(&format!("Cannot {} path: {}\n\n", action, path_text));

        // Add permission information
        output.push_str("Please check:\n");
        output.push_str(&format!(
            "  1. That you have permission to {} this path\n",
            action
        ));
        output.push_str("  2. That the path exists and is accessible\n");
        output.push_str("  3. You may need to run with elevated privileges\n");

        output
    }

    /// Format a circular dependency error
    pub fn format_circular_dependency(&self, cycle: &[String]) -> String {
        let mut output = String::new();

        let error_header = self.format_header("Circular dependency detected", MessageType::Error);
        output.push_str(&error_header);

        output.push_str("A circular dependency was detected in your package definitions.\n\n");

        // Format the dependency cycle
        output.push_str("Dependency cycle:\n");

        let cycle_text = if self.progress_manager.use_colors() {
            let packages: Vec<String> = cycle
                .iter()
                .map(|pkg| style(pkg).magenta().to_string())
                .collect();
            packages.join(" → ")
        } else {
            cycle.join(" → ")
        };

        output.push_str(&format!("  {}\n\n", cycle_text));

        output.push_str("Please modify your package definitions to remove this cycle.\n");

        output
    }

    /// Format a validation result
    pub fn format_validation(&self, result: &ValidationResult) -> String {
        // Use the existing validation formatter
        result.format_validation_result(self.progress_manager)
    }

    /// Format a section header
    fn format_header(&self, text: &str, message_type: MessageType) -> String {
        if self.progress_manager.use_colors() {
            match message_type {
                MessageType::Error => {
                    format!("{}\n\n", style(format!("Error: {}", text)).red().bold())
                }
                MessageType::Warning => format!(
                    "{}\n\n",
                    style(format!("Warning: {}", text)).yellow().bold()
                ),
                _ => format!("{}\n\n", style(text).bold()),
            }
        } else {
            format!("{}: {}\n\n", message_type.as_str(), text)
        }
    }
}

// Helper extension for MessageType
impl MessageType {
    fn as_str(&self) -> &str {
        match self {
            MessageType::Info => "Info",
            MessageType::Success => "Success",
            MessageType::Error => "Error",
            MessageType::Warning => "Warning",
            MessageType::Loading => "Loading",
            MessageType::Status => "Status",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_format_package_not_found() {
        let progress_manager = ProgressManager::new(false, false, true);
        let formatter = ErrorFormatter::new(&progress_manager);

        let suggestions = vec!["ripgrep".to_string(), "ripgrep-all".to_string()];
        let output = formatter.format_package_not_found("rigrep", &suggestions);

        // Check for essential elements in the error message
        assert!(output.contains("Error: Package not found"));
        assert!(output.contains("Cannot find package: rigrep"));
        assert!(output.contains("Did you mean:"));
        assert!(output.contains("ripgrep"));
        assert!(output.contains("ripgrep-all"));
        assert!(output.contains("selfie package list"));
    }

    #[test]
    fn test_format_command_error() {
        let progress_manager = ProgressManager::new(false, false, true);
        let formatter = ErrorFormatter::new(&progress_manager);

        let output = formatter.format_command_error(
            "brew install ripgrep",
            1,
            "Starting installation...",
            "Error: Package not found",
        );

        // Check for essential elements in the error message
        assert!(output.contains("Error: Command execution failed"));
        assert!(output.contains("Command: 'brew install ripgrep'"));
        assert!(output.contains("Exit code: 1"));
        assert!(output.contains("Standard output:"));
        assert!(output.contains("Starting installation..."));
        assert!(output.contains("Error output:"));
        assert!(output.contains("Error: Package not found"));
    }

    #[test]
    fn test_format_circular_dependency() {
        let progress_manager = ProgressManager::new(false, false, true);
        let formatter = ErrorFormatter::new(&progress_manager);

        let cycle = vec![
            "package-a".to_string(),
            "package-b".to_string(),
            "package-c".to_string(),
            "package-a".to_string(),
        ];
        let output = formatter.format_circular_dependency(&cycle);

        // Check for essential elements in the error message
        assert!(output.contains("Error: Circular dependency detected"));
        assert!(output.contains("Dependency cycle:"));
        assert!(output.contains("package-a → package-b → package-c → package-a"));
    }

    #[test]
    fn test_format_permission_error() {
        let progress_manager = ProgressManager::new(false, false, true);
        let formatter = ErrorFormatter::new(&progress_manager);

        let path = PathBuf::from("/protected/file.txt");
        let output = formatter.format_permission_error(&path, "read");

        // Check for essential elements in the error message
        assert!(output.contains("Error: Permission denied"));
        assert!(output.contains("Cannot read path: /protected/file.txt"));
        assert!(output.contains("That you have permission to read this path"));
    }
}
