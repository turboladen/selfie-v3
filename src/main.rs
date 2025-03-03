// src/main.rs
// Update the ProgressManager initialization to enable colors by default

use std::{process, time::Duration};

use selfie::adapters::package_repo::yaml::YamlPackageRepository;
use selfie::{
    adapters::command::shell::ShellCommandRunner,
    adapters::filesystem::RealFileSystem,
    cli::{self, Cli, Commands, PackageSubcommands},
    progress_display::ProgressManager,
    services::multi_package_installation_service::MultiPackageInstallationService,
    services::package_list_service::{PackageListResult, PackageListService},
    services::package_validation_service::{PackageValidationResult, PackageValidationService},
};

fn main() {
    // Parse command line arguments
    let cli = Cli::parse_args();

    // Create a progress manager - CHANGED HERE: use !cli.no_color to enable colors by default
    let progress_manager = ProgressManager::new(!cli.no_color, true, cli.verbose);

    // Create a base configuration (in a real app, this would be loaded from a file)
    let base_config = None;

    // Set up file system and command runner
    let fs = RealFileSystem;
    let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(60));

    // Display command that will be executed
    let cmd_desc = cli::get_command_description(&cli);
    let info_pb = progress_manager.create_progress_bar(
        "info",
        &cmd_desc,
        selfie::progress_display::ProgressStyleType::Message,
    );
    info_pb.finish();

    // Execute the command
    let result = match &cli.command {
        Commands::Package(pkg_cmd) => {
            match &pkg_cmd.command {
                PackageSubcommands::Install { package_name } => {
                    // For install commands, we need a fully valid config
                    match cli.validate_and_build_config(base_config) {
                        Ok(config) => {
                            // Use the enhanced installer with progress display
                            let installer = MultiPackageInstallationService::new(
                                &fs,
                                &runner,
                                &config,
                                cli.verbose,
                                !cli.no_color, // CHANGED: enable colors by default
                                true,          // use_unicode
                            );

                            match installer.install_package(package_name) {
                                Ok(_) => 0,
                                Err(err) => {
                                    let error_pb = progress_manager.create_progress_bar(
                                        "error",
                                        &format!("Installation failed: {}", err),
                                        selfie::progress_display::ProgressStyleType::Message,
                                    );
                                    error_pb.abandon();
                                    1
                                }
                            }
                        }
                        Err(err) => {
                            // The progress already showed a failure indicator
                            // Just print the error details
                            eprintln!("Error: {}", err);
                            1
                        }
                    }
                }
                PackageSubcommands::List => {
                    // For list commands, we only need a minimal config validation
                    match cli.build_minimal_config(base_config) {
                        Ok(config) => {
                            let package_repo = YamlPackageRepository::new(
                                &fs,
                                config.expanded_package_directory(),
                            );
                            let list_cmd = PackageListService::new(
                                &runner,
                                &config,
                                &progress_manager,
                                cli.verbose,
                                &package_repo,
                            );

                            match list_cmd.execute() {
                                PackageListResult::Success(output) => {
                                    // The progress bar already showed "Done"
                                    // Just print the package list
                                    println!("{}", output);
                                    0
                                }
                                PackageListResult::Error(error) => {
                                    // The progress bar already showed "Failed"
                                    // Print the detailed error
                                    eprintln!("{}", error);
                                    1
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("Error: {}", err);
                            eprintln!("\nPackage directory is required for listing packages.");
                            eprintln!("You can set it with:");
                            eprintln!("  1. The --package-directory flag: --package-directory /path/to/packages");
                            eprintln!("  2. In your config.yaml file: package_directory: /path/to/packages");
                            1
                        }
                    }
                }
                PackageSubcommands::Info { package_name } => {
                    let info_pb = progress_manager.create_progress_bar(
                        "info",
                        &format!("Package info for '{}' not implemented yet", package_name),
                        selfie::progress_display::ProgressStyleType::Message,
                    );
                    info_pb.finish();
                    0
                }
                PackageSubcommands::Create { package_name } => {
                    let info_pb = progress_manager.create_progress_bar(
                        "create",
                        &format!(
                            "Package creation for '{}' not implemented yet",
                            package_name
                        ),
                        selfie::progress_display::ProgressStyleType::Message,
                    );
                    info_pb.finish();
                    0
                }
                PackageSubcommands::Validate { .. } => {
                    // For validation commands, use minimal config validation that only checks package directory
                    match cli.build_minimal_config(base_config) {
                        Ok(config) => {
                            // Use the validate command
                            let validate_cmd = PackageValidationService::new(
                                &fs,
                                &runner,
                                config,
                                &progress_manager,
                                cli.verbose,
                            );

                            match validate_cmd.execute(&pkg_cmd.command) {
                                PackageValidationResult::Valid(output) => {
                                    // The progress bar already showed "Validation successful"
                                    // Just print the details now
                                    println!("{}", output);
                                    0
                                }
                                PackageValidationResult::Invalid(output) => {
                                    // The progress bar already showed "Validation failed"
                                    // Just print the details now
                                    println!("{}", output);
                                    1
                                }
                                PackageValidationResult::Error(error) => {
                                    // The progress bar already showed a generic failure message
                                    // Print the detailed error to stderr
                                    eprintln!("{}", error);
                                    1
                                }
                            }
                        }
                        Err(err) => {
                            eprintln!("Error: {}", err);
                            eprintln!("\nPackage directory is required for validation.");
                            eprintln!("You can set it with:");
                            eprintln!("  1. The --package-directory flag: --package-directory /path/to/packages");
                            eprintln!("  2. In your config.yaml file: package_directory: /path/to/packages");
                            1
                        }
                    }
                }
            }
        }
        Commands::Config(_cfg_cmd) => {
            let info_pb = progress_manager.create_progress_bar(
                "config",
                "Config commands not implemented yet",
                selfie::progress_display::ProgressStyleType::Message,
            );
            info_pb.finish();
            0
        }
    };

    process::exit(result);
}
