// src/services/enhanced_error_handler.rs
// Combines error formatting and suggestions into a comprehensive error handling system

use std::error::Error;
use std::path::Path;

use crate::adapters::progress::{MessageType, ProgressManager};
use crate::domain::validation::ValidationResult;
use crate::ports::filesystem::FileSystem;
use crate::ports::package_repo::PackageRepository;
use crate::services::error_formatter::ErrorFormatter;
use crate::services::suggestion_provider::SuggestionProvider;

/// Enhanced error handler that provides rich contextual error information
pub struct EnhancedErrorHandler<'a, F: FileSystem, P: PackageRepository> {
    fs: &'a F,
    progress_manager: &'a ProgressManager,
    formatter: ErrorFormatter<'a>,
    suggestion_provider: SuggestionProvider<'a, F, P>,
}

impl<'a, F: FileSystem, P: PackageRepository> EnhancedErrorHandler<'a, F, P> {
    /// Create a new enhanced error handler
    pub fn new(fs: &'a F, package_repo: &'a P, progress_manager: &'a ProgressManager) -> Self {
        Self {
            fs,
            progress_manager,
            formatter: ErrorFormatter::new(progress_manager),
            suggestion_provider: SuggestionProvider::new(fs, package_repo),
        }
    }

    /// Handle package not found errors with suggestions
    pub fn handle_package_not_found(&self, name: &str) -> String {
        // Get suggestions for the package name using the stored package repository
        let suggestions = self.suggestion_provider.suggest_package(name);

        // Format the error message with suggestions
        self.formatter.format_package_not_found(name, &suggestions)
    }

    /// Handle command execution errors
    pub fn handle_command_error(
        &self,
        command: &str,
        exit_code: i32,
        stdout: &str,
        stderr: &str,
    ) -> String {
        self.formatter
            .format_command_error(command, exit_code, stdout, stderr)
    }

    /// Handle config errors
    pub fn handle_config_error(&self, error: &dyn Error) -> String {
        self.formatter.format_config_error(error)
    }

    /// Handle dependency errors
    pub fn handle_dependency_error(&self, error: &dyn Error, dependencies: &[String]) -> String {
        self.formatter.format_dependency_error(error, dependencies)
    }

    /// Handle validation errors
    pub fn handle_validation_error(&self, result: &ValidationResult) -> String {
        self.formatter.format_validation(result)
    }

    /// Handle circular dependency errors
    pub fn handle_circular_dependency(&self, cycle: &[String]) -> String {
        self.formatter.format_circular_dependency(cycle)
    }

    /// Handle path not found errors with suggestions
    pub fn handle_path_not_found(&self, path: &Path) -> String {
        let suggestions = self.suggestion_provider.suggest_path(path);
        let mut message = format!("Path not found: {}", path.display());

        if !suggestions.is_empty() {
            message.push_str("\n\nDid you mean:");
            for suggestion in suggestions {
                message.push_str(&format!("\n  • {}", suggestion.display()));
            }
        }

        if let Some(parent) = path.parent() {
            if !self.fs.path_exists(parent) {
                message.push_str(&format!(
                    "\n\nParent directory doesn't exist: {}",
                    parent.display()
                ));
                message.push_str("\nYou may need to create this directory first.");
            }
        }

        message
    }

    /// Handle general errors with context extraction
    pub fn handle_error(&self, error: &dyn Error) -> String {
        // Analyze the error string to see if we can provide more specific handling
        let error_text = error.to_string().to_lowercase();

        // Package not found errors
        if error_text.contains("package not found") {
            // Try to extract package name from error message
            if let Some(name) = self.extract_quoted_text(&error_text) {
                return self.handle_package_not_found(&name);
            }
        }

        // Path not found errors
        if error_text.contains("path not found") || error_text.contains("no such file") {
            // Try to extract path from error message
            if let Some(path_str) = self.extract_quoted_text(&error_text) {
                return self.handle_path_not_found(Path::new(&path_str));
            }
        }

        // Command execution errors
        if error_text.contains("command") && error_text.contains("failed") {
            // If we can't extract detailed command info, at least provide
            // a general command error message
            return self.progress_manager.status_line(
                MessageType::Error,
                &format!("Command execution failed: {}", error),
            );
        }

        // Circular dependency errors
        if error_text.contains("circular dependency") {
            return self.progress_manager.status_line(
                MessageType::Error,
                &format!("Circular dependency detected: {}", error),
            );
        }

        // For any other error, provide a generic formatted message
        self.progress_manager
            .status_line(MessageType::Error, &error.to_string())
    }

    /// Extract text within quotes from an error message
    fn extract_quoted_text(&self, text: &str) -> Option<String> {
        let parts: Vec<&str> = text.split('\'').collect();
        if parts.len() >= 3 {
            // Text within the first set of quotes
            return Some(parts[1].to_string());
        }

        let parts: Vec<&str> = text.split('"').collect();
        if parts.len() >= 3 {
            // Text within the first set of double quotes
            return Some(parts[1].to_string());
        }

        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::package::PackageBuilder;
    use crate::ports::{filesystem::MockFileSystem, package_repo::MockPackageRepository};

    #[test]
    fn test_handle_package_not_found() {
        let fs = MockFileSystem::default();
        let mut package_repo = MockPackageRepository::new();
        let progress_manager = ProgressManager::new(false, false, true);

        // Create test packages
        let packages = vec![
            PackageBuilder::default()
                .name("ripgrep")
                .version("1.0.0")
                .build(),
            PackageBuilder::default()
                .name("ripgrep-all")
                .version("1.0.0")
                .build(),
            PackageBuilder::default()
                .name("fzf")
                .version("1.0.0")
                .build(),
        ];

        package_repo
            .expect_list_packages()
            .returning(move || Ok(packages.clone()));

        let handler = EnhancedErrorHandler::new(&fs, &package_repo, &progress_manager);

        // Test error message with suggestion
        let error_msg = handler.handle_package_not_found("rigrep");
        assert!(error_msg.contains("Package not found"));
        assert!(error_msg.contains("rigrep"));
        assert!(error_msg.contains("Did you mean:"));
        assert!(error_msg.contains("ripgrep"));

        // Test error message without suggestion
        let error_msg = handler.handle_package_not_found("something-completely-different");
        assert!(error_msg.contains("Package not found"));
        assert!(error_msg.contains("something-completely-different"));
        assert!(!error_msg.contains("Did you mean:"));
    }

    #[test]
    fn test_handle_path_not_found() {
        let mut fs = MockFileSystem::default();
        let package_repo = MockPackageRepository::new();
        let progress_manager = ProgressManager::new(false, false, true);

        // Set up mock filesystem
        let dir = Path::new("/test/dir");
        fs.mock_path_exists(dir, true);
        fs.expect_list_directory()
            .with(mockall::predicate::eq(dir))
            .returning(|_| Ok(vec![dir.join("config.yaml")]));
        fs.mock_path_exists(dir.join("config.yaml"), true);
        fs.mock_path_exists(dir.join("config.yml"), false);

        fs.mock_path_exists("/nonexistent/dir", false);
        fs.mock_path_exists("/nonexistent/dir/file.txt", false);

        let handler = EnhancedErrorHandler::new(&fs, &package_repo, &progress_manager);

        // Test error with suggestion
        let error_msg = handler.handle_path_not_found(&dir.join("config.yml"));
        dbg!(&error_msg);
        assert!(error_msg.contains("Path not found"));
        assert!(error_msg.contains("Did you mean:"));
        assert!(error_msg.contains("config.yaml"));

        // Test error when parent doesn't exist
        let error_msg = handler.handle_path_not_found(Path::new("/nonexistent/dir/file.txt"));
        assert!(error_msg.contains("Path not found"));
        assert!(error_msg.contains("Parent directory doesn't exist"));
    }

    #[test]
    fn test_handle_circular_dependency() {
        let fs = MockFileSystem::default();
        let package_repo = MockPackageRepository::new();
        let progress_manager = ProgressManager::new(false, false, true);

        let handler = EnhancedErrorHandler::new(&fs, &package_repo, &progress_manager);

        let cycle = vec![
            "package-a".to_string(),
            "package-b".to_string(),
            "package-a".to_string(),
        ];
        let error_msg = handler.handle_circular_dependency(&cycle);

        assert!(error_msg.contains("Circular dependency detected"));
        assert!(error_msg.contains("package-a"));
        assert!(error_msg.contains("package-b"));
    }

    #[test]
    fn test_extract_quoted_text() {
        let fs = MockFileSystem::default();
        let package_repo = MockPackageRepository::new();
        let progress_manager = ProgressManager::new(false, false, true);

        let handler = EnhancedErrorHandler::new(&fs, &package_repo, &progress_manager);

        // Test with single quotes
        assert_eq!(
            handler.extract_quoted_text("Package 'test-package' not found"),
            Some("test-package".to_string())
        );

        // Test with double quotes
        assert_eq!(
            handler.extract_quoted_text("Path \"config.yaml\" not found"),
            Some("config.yaml".to_string())
        );

        // Test with no quotes
        assert_eq!(handler.extract_quoted_text("Error occurred"), None);
    }
}
