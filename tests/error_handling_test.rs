// tests/error_handling_test.rs
// Integration tests for enhanced error handling

use std::path::Path;

use selfie::{
    adapters::progress::ProgressManager,
    domain::errors::{EnhancedPackageError, ErrorContext},
    ports::{
        filesystem::{MockFileSystem, MockFileSystemExt},
        package_repo::MockPackageRepository,
    },
    services::{
        enhanced_error_handler::EnhancedErrorHandler, suggestion_provider::SuggestionProvider,
    },
};

#[test]
fn test_enhanced_error_handler_package_not_found() {
    // Set up test environment
    let mut fs = MockFileSystem::default();
    let mut package_repo = MockPackageRepository::new();
    let progress_manager = ProgressManager::new(false, true, false);

    // Add some package files
    let yaml = r#"
        name: ripgrep
        version: 1.0.0
        environments:
          test-env:
            install: brew install ripgrep
    "#;

    fs.add_file(Path::new("/test/packages/ripgrep.yaml"), yaml);

    // Set up package repository to return 'not found' for a nonexistent package
    package_repo
        .expect_get_package()
        .with(mockall::predicate::eq("nonexistent"))
        .returning(|_| {
            Err(
                selfie::ports::package_repo::PackageRepoError::PackageNotFound(
                    "nonexistent".to_string(),
                ),
            )
        });

    // Set up suggestion provider
    package_repo.expect_list_packages().returning(move || {
        Ok(vec![selfie::domain::package::PackageBuilder::default()
            .name("ripgrep")
            .version("1.0.0")
            .build()])
    });

    // Create the error handler
    let error_handler = EnhancedErrorHandler::new(&fs, &package_repo, &progress_manager);

    // Test the package not found error
    let error_message = error_handler.handle_package_not_found("rigrep");

    // The error should contain the package name and a suggestion
    assert!(error_message.contains("Package not found"));
    assert!(error_message.contains("rigrep"));
    assert!(error_message.contains("ripgrep"));
}

#[test]
fn test_enhanced_error_with_context() {
    // Create an enhanced error with context
    let error = EnhancedPackageError::package_not_found("test-package").with_context(
        ErrorContext::new()
            .with_environment("test-env")
            .with_command("install command")
            .with_path("/test/path"),
    );

    // The error should contain the package name
    assert!(error
        .to_string()
        .contains("Package not found: test-package"));

    // The context should contain all the details
    let context = error.context();
    assert_eq!(context.environment.as_deref(), Some("test-env"));
    assert_eq!(context.command.as_deref(), Some("install command"));
    assert_eq!(
        context
            .path
            .as_ref()
            .map(|p| p.to_string_lossy().to_string()),
        Some("/test/path".to_string())
    );
}

#[test]
fn test_suggestion_provider_package_names() {
    // Set up test environment
    let fs = MockFileSystem::default();
    let mut package_repo = MockPackageRepository::new();

    // Set up package repository to list some packages
    package_repo.expect_list_packages().returning(move || {
        Ok(vec![
            selfie::domain::package::PackageBuilder::default()
                .name("ripgrep")
                .version("1.0.0")
                .build(),
            selfie::domain::package::PackageBuilder::default()
                .name("ripgrep-all")
                .version("1.0.0")
                .build(),
            selfie::domain::package::PackageBuilder::default()
                .name("fzf")
                .version("1.0.0")
                .build(),
        ])
    });

    // Create the suggestion provider
    let provider = SuggestionProvider::new(&fs, &package_repo);

    // Test suggestion for a misspelled package name
    let suggestions = provider.suggest_package("rigrep");
    assert!(!suggestions.is_empty());
    assert!(suggestions.contains(&"ripgrep".to_string()));

    // Test suggestion for a very different name
    let suggestions = provider.suggest_package("xyz");
    assert!(suggestions.is_empty());
}

#[test]
fn test_error_context_formatting() {
    // Create an error context with multiple fields
    let context = ErrorContext::new()
        .with_package("test-package")
        .with_environment("test-env")
        .with_command("test command")
        .with_line(42)
        .with_message("Additional context");

    // The string representation should contain all fields
    let context_str = context.to_string();
    assert!(context_str.contains("Package: test-package"));
    assert!(context_str.contains("Environment: test-env"));
    assert!(context_str.contains("Command: test command"));
    assert!(context_str.contains("Line: 42"));
    assert!(context_str.contains("Additional context"));
}
