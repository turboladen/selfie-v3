// tests/progress_display_test.rs

use selfie::{
    command::mock::MockCommandRunner,
    config::ConfigBuilder,
    filesystem::mock::MockFileSystem,
    installation_manager::InstallationStatus,
    package_installer::PackageInstaller,
    progress_display::{ProgressManager, ProgressStyleType},
};

// Import the enhanced package installer
use std::path::Path;

#[test]
fn test_package_install_with_progress_display() {
    // Create mock environment
    let fs = MockFileSystem::default();
    let runner = MockCommandRunner::new();

    // Create config
    let config = ConfigBuilder::default()
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

    fs.add_file(Path::new("/test/packages/main-pkg.yaml"), main_package_yaml);
    fs.add_file(Path::new("/test/packages/dep1.yaml"), dep1_yaml);
    fs.add_file(Path::new("/test/packages/dep2.yaml"), dep2_yaml);
    fs.add_existing_path(Path::new("/test/packages"));

    // Set up mock command responses
    // None of the packages are installed
    runner.error_response("main check", "Not found", 1);
    runner.success_response("main install", "Installed successfully");
    runner.error_response("dep1 check", "Not found", 1);
    runner.success_response("dep1 install", "Installed successfully");
    runner.error_response("dep2 check", "Not found", 1);
    runner.success_response("dep2 install", "Installed successfully");

    // Create the enhanced installer
    let installer = PackageInstaller::new(
        fs, runner, config, true,  // verbose output
        false, // no colors
        true,  // use unicode
    );

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

    // Complete a progress bar
    progress_manager
        .complete_progress("spinner-test", "Completed spinner")
        .unwrap();

    // Update using installation status
    progress_manager
        .update_from_status(
            "bar-test",
            &InstallationStatus::Complete,
            Some(std::time::Duration::from_millis(123)),
        )
        .unwrap();

    // Test getting a non-existent progress bar
    assert!(progress_manager.get_progress_bar("nonexistent").is_none());
    assert!(progress_manager
        .update_progress("nonexistent", "test")
        .is_err());

    // Finish test
    message_pb.finish_with_message("Test completed");
}
