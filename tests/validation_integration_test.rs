// Test for integration in tests/validation_integration_test.rs
// This verifies the full validation flow

use selfie::{
    adapters::progress::ProgressManager,
    domain::config::AppConfigBuilder,
    ports::{command::MockCommandRunner, filesystem::MockFileSystem},
    services::validation_command::{ValidationCommand, ValidationCommandResult},
};
use std::path::Path;

#[test]
fn test_validation_integration() {
    // Set up test environment
    let mut fs = MockFileSystem::default();
    let mut runner = MockCommandRunner::new();

    // Create config
    let config = AppConfigBuilder::default()
        .environment("test-env")
        .package_directory("/test/packages")
        .build();

    // Set up package directory
    let package_dir = Path::new("/test/packages");
    fs.mock_path_exists(&package_dir, true);

    // Create a valid package file
    let valid_yaml = r#"
        name: valid-package
        version: 1.0.0
        homepage: https://example.com
        description: A valid test package
        environments:
          test-env:
            install: echo test
            check: which test
    "#;

    let valid_path = package_dir.join("valid-package.yaml");
    fs.mock_path_exists(&valid_path, true);
    fs.mock_path_exists(package_dir.join("valid-package.yml"), false);
    fs.mock_read_file(&valid_path, valid_yaml);

    // Create an invalid package file
    let invalid_yaml = r#"
        name: ""
        version: ""
        homepage: invalid-url
        environments:
          other-env:
            install: ""
    "#;

    let invalid_path = package_dir.join("invalid-package.yaml");
    fs.mock_path_exists(&invalid_path, true);
    fs.mock_path_exists(package_dir.join("invalid-package.yml"), false);
    fs.mock_read_file(&invalid_path, invalid_yaml);

    // Set up command runner
    runner.mock_is_command_available("echo", true);

    runner.mock_is_command_available("which", true);

    // Set up package repository for file finding
    // let package_repo = YamlPackageRepository::new(&fs, config.expanded_package_directory());

    // Create progress manager
    let progress_manager = ProgressManager::new(false, false, false);

    // Create validation command
    let command = ValidationCommand::new(&fs, &runner, &config, &progress_manager);

    // Test validation on valid package
    let valid_cmd = selfie::domain::application::commands::PackageCommand::Validate {
        package_name: "valid-package".to_string(),
        package_path: None,
    };

    let result = command.execute(&valid_cmd);
    match result {
        ValidationCommandResult::Valid(output) => {
            assert!(output.contains("valid-package"));
            assert!(output.contains("is valid"));
        }
        _ => panic!("Expected Valid result"),
    }

    // Test validation on invalid package
    let invalid_cmd = selfie::domain::application::commands::PackageCommand::Validate {
        package_name: "invalid-package".to_string(),
        package_path: None,
    };

    let result = command.execute(&invalid_cmd);
    match result {
        ValidationCommandResult::Invalid(output) => {
            eprintln!("{}", &output);
            assert!(output.contains("invalid-package"));
            assert!(output.contains("Validation failed"));
            assert!(output.contains("Required field errors"));
            assert!(output.contains("URL format errors"));
        }
        _ => panic!("Expected Invalid result"),
    }

    // Test validation with path parameter
    let path_cmd = selfie::domain::application::commands::PackageCommand::Validate {
        package_name: "any-name".to_string(),
        package_path: Some(valid_path),
    };

    let result = command.execute(&path_cmd);
    match result {
        ValidationCommandResult::Valid(_) => {
            // Expected result
        }
        _ => panic!("Expected Valid result for path validation"),
    }
}
