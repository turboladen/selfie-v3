// tests/install_command_test.rs

use std::path::Path;

use selfie::{
    adapters::progress::ProgressManager,
    domain::{config::AppConfigBuilder, installation::InstallationStatus},
    ports::{
        command::{MockCommandRunner, MockCommandRunnerExt},
        filesystem::MockFileSystem,
    },
    services::package_installer::PackageInstaller,
};

#[test]
fn test_package_install_end_to_end() {
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

    let progress_manager = ProgressManager::new(false, true, true);

    // Create package installer (using the new consolidated version)
    let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, false);

    // Run the installation
    let result = installer.install_package("ripgrep");

    // Verify the result
    assert!(result.is_ok());
    let install_result = result.unwrap();
    assert_eq!(install_result.package_name, "ripgrep");
}

#[test]
fn test_package_install_with_dependencies() {
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

    let progress_manager = ProgressManager::new(false, true, true);

    // Create package installer
    let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, false);

    // Run the installation
    let result = installer.install_package("ripgrep");

    // Verify the result
    assert!(result.is_ok());
    let install_result = result.unwrap();
    assert_eq!(install_result.package_name, "ripgrep");
    assert_eq!(install_result.dependencies.len(), 1);
    assert_eq!(install_result.dependencies[0].package_name, "rust");
}

// Update the test in tests/integration_test.rs to test dependency resolution

#[test]
fn test_package_install_with_complex_dependencies() {
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

    let progress_manager = ProgressManager::new(false, true, true);

    // Create package installer
    let installer = PackageInstaller::new(&fs, &runner, &config, &progress_manager, false);

    // Run the installation
    let result = installer.install_package("main-pkg");

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
