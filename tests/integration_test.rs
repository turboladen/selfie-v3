// tests/install_command_test.rs


use selfie::{
    command::mock::MockCommandRunner,
    config::ConfigBuilder,
    filesystem::mock::MockFileSystem,
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
