// tests/progress_display_test.rs

use selfie::{
    adapters::progress::{ProgressManager, ProgressStyleType},
    domain::{config::AppConfigBuilder, installation::InstallationStatus},
    ports::{
        command::{MockCommandRunner, MockCommandRunnerExt},
        filesystem::MockFileSystem,
    },
    services::package_installer::PackageInstaller,
};

// Import the enhanced package installer
use std::path::Path;

#[test]
fn test_package_install_with_progress_display() {
    // Create mock environment
    let mut fs = MockFileSystem::default();
    let mut runner = MockCommandRunner::new();

    // Create config
    let config = AppConfigBuilder::default()
        .environment("test-env")
        .package_directory("/test/packages")
        .build();
    // Set up package files in the filesystem

    // Main package with two dependencies
    let main_package_yaml = r#"
        name: main-pkg
        version: 1.0.0
        environments:
          test-env:
            install: main install
            check: main check
            dependencies:
              - dep1
              - dep2
    "#;

    // Dependency 1
    let dep1_yaml = r#"
        name: dep1
        version: 1.0.0
        environments:
          test-env:
            install: dep1 install
            check: dep1 check
    "#;

    // Dependency 2
    let dep2_yaml = r#"
        name: dep2
        version: 1.0.0
        environments:
          test-env:
            install: dep2 install
            check: dep2 check
    "#;

    let package_dir = Path::new("/test/packages");
    fs.mock_path_exists(&package_dir, true);

    let main_pkg = package_dir.join("main-pkg.yaml");
    fs.mock_path_exists(&main_pkg, true);
    fs.mock_path_exists(package_dir.join("main-pkg.yml"), false);
    fs.mock_read_file(&main_pkg, main_package_yaml);

    let dep1 = package_dir.join("dep1.yaml");
    fs.mock_path_exists(&dep1, true);
    fs.mock_path_exists(package_dir.join("dep1.yml"), false);
    fs.mock_read_file(&dep1, dep1_yaml);

    let dep2 = package_dir.join("dep2.yaml");
    fs.mock_path_exists(&dep2, true);
    fs.mock_path_exists(package_dir.join("dep2.yml"), false);
    fs.mock_read_file(&dep2, dep2_yaml);

    // Set up mock command responses
    // None of the packages are installed
    runner.error_response("main check", "Not found", 1);
    runner.success_response("main install", "Installed successfully");
    runner.error_response("dep1 check", "Not found", 1);
    runner.success_response("dep1 install", "Installed successfully");
    runner.error_response("dep2 check", "Not found", 1);
    runner.success_response("dep2 install", "Installed successfully");
    runner.mock_is_command_available("dep1", true);
    runner.mock_is_command_available("dep2", true);
    runner.mock_is_command_available("main", true);

    let progress_manager = ProgressManager::new(false, true, true);

    // Create the enhanced installer
    let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, true);

    // Run the installation
    let result = installer.install_package("main-pkg");

    // Verify the result
    assert!(result.is_ok());
    let install_result = result.unwrap();

    // Check that the main package was installed successfully
    assert_eq!(install_result.package_name, "main-pkg");
    assert_eq!(install_result.status, InstallationStatus::Complete);

    // Check that both dependencies were installed
    assert_eq!(install_result.dependencies.len(), 2);

    // Find and verify each dependency
    let dep1_found = install_result
        .dependencies
        .iter()
        .any(|dep| dep.package_name == "dep1" && dep.status == InstallationStatus::Complete);
    let dep2_found = install_result
        .dependencies
        .iter()
        .any(|dep| dep.package_name == "dep2" && dep.status == InstallationStatus::Complete);

    assert!(
        dep1_found,
        "Dependency 'dep1' not found or not successfully installed"
    );
    assert!(
        dep2_found,
        "Dependency 'dep2' not found or not successfully installed"
    );

    // Check that duration information is present
    assert!(install_result.duration.as_micros() > 0);
    assert!(install_result.total_duration().as_micros() > 0);
    assert!(install_result.dependency_duration().as_micros() > 0);
}

#[test]
fn test_direct_progress_display_usage() {
    // Test that the ProgressManager works directly
    let progress_manager = ProgressManager::new(false, true, true);

    // Create a few progress bars with different styles
    let spinner_pb = progress_manager.create_progress_bar(
        "spinner-test",
        "Testing spinner",
        ProgressStyleType::Spinner,
    );

    let bar_pb =
        progress_manager.create_progress_bar("bar-test", "Testing bar", ProgressStyleType::Bar);

    let message_pb = progress_manager.create_progress_bar(
        "message-test",
        "Testing message",
        ProgressStyleType::Message,
    );

    // Update the progress bars
    spinner_pb.set_message("Updated spinner message");
    progress_manager
        .update_progress("bar-test", "Updated bar message")
        .unwrap();

    // Add some progress to the bar
    bar_pb.set_position(50);

    let info_message = progress_manager.info("Information message");
    assert!(info_message.contains("Information message"));

    let success_message = progress_manager.success("Success message");
    assert!(success_message.contains("Success message"));

    let error_message = progress_manager.error("Error message");
    assert!(error_message.contains("Error message"));

    // Complete a progress bar
    progress_manager
        .complete_progress("spinner-test", "Completed spinner")
        .unwrap();

    // Finish test
    message_pb.finish_with_message("Test completed");
}
