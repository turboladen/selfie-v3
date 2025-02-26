// tests/install_command_test.rs

use selfie::{
    command::mock::MockCommandRunner,
    config::ConfigBuilder,
    filesystem::mock::MockFileSystem,
    installation::InstallationStatus,
    package_installer::PackageInstaller,
    progress::{ConsoleRenderer, ProgressReporter},
};

#[test]
fn test_package_install_end_to_end() {
    // Create mock environment
    let fs = MockFileSystem::default();
    let runner = MockCommandRunner::new();
    let reporter = ProgressReporter::new(Box::new(ConsoleRenderer::new(false, false)));

    // Create config
    let config = ConfigBuilder::default()
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

    fs.add_file(
        std::path::Path::new("/test/packages/ripgrep.yaml"),
        package_yaml,
    );
    fs.add_existing_path(std::path::Path::new("/test/packages"));

    // Set up mock command responses
    runner.error_response("rg check", "Not found", 1); // Not installed
    runner.success_response("rg install", "Installed successfully");

    // Create package installer
    let installer = PackageInstaller::new(fs, runner, config, reporter.clone(), false);

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
    let fs = MockFileSystem::default();
    let runner = MockCommandRunner::new();
    let reporter = ProgressReporter::new(Box::new(ConsoleRenderer::new(false, false)));

    // Create config
    let config = ConfigBuilder::default()
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

    fs.add_file(
        std::path::Path::new("/test/packages/ripgrep.yaml"),
        package_yaml,
    );
    fs.add_file(
        std::path::Path::new("/test/packages/rust.yaml"),
        dependency_yaml,
    );
    fs.add_existing_path(std::path::Path::new("/test/packages"));

    // Set up mock command responses
    runner.error_response("rg check", "Not found", 1); // Not installed
    runner.success_response("rg install", "Installed successfully");
    runner.error_response("rust check", "Not found", 1); // Not installed
    runner.success_response("rust install", "Installed successfully");

    // Create package installer
    let installer = PackageInstaller::new(fs, runner, config, reporter.clone(), false);

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
    let fs = MockFileSystem::default();
    let runner = MockCommandRunner::new();
    let reporter = ProgressReporter::new(Box::new(ConsoleRenderer::new(false, false)));

    // Create config
    let config = ConfigBuilder::default()
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

    fs.add_file(
        std::path::Path::new("/test/packages/main-pkg.yaml"),
        main_pkg_yaml,
    );
    fs.add_file(std::path::Path::new("/test/packages/dep1.yaml"), dep1_yaml);
    fs.add_file(std::path::Path::new("/test/packages/dep2.yaml"), dep2_yaml);
    fs.add_file(std::path::Path::new("/test/packages/dep3.yaml"), dep3_yaml);
    fs.add_existing_path(std::path::Path::new("/test/packages"));

    // Set up mock command responses - all need to be installed
    runner.error_response("main-check", "Not found", 1);
    runner.success_response("main-install", "Installed successfully");
    runner.error_response("dep1-check", "Not found", 1);
    runner.success_response("dep1-install", "Installed successfully");
    runner.error_response("dep2-check", "Not found", 1);
    runner.success_response("dep2-install", "Installed successfully");
    runner.error_response("dep3-check", "Not found", 1);
    runner.success_response("dep3-install", "Installed successfully");

    // Create package installer
    let installer = PackageInstaller::new(fs, runner, config, reporter, false);

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
