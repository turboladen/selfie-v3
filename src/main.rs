// src/main.rs

use std::{process, time::Duration};

use selfie::{
    cli::{self, Cli, Commands, PackageSubcommands},
    command::ShellCommandRunner,
    filesystem::RealFileSystem,
    package_installer::PackageInstaller,
    progress_display::ProgressManager,
};

fn main() {
    // Parse command line arguments
    let cli = Cli::parse_args();

    // Create a progress manager
    let progress_manager = ProgressManager::new(!cli.no_color, true, cli.verbose);

    // Create a base configuration (in a real app, this would be loaded from a file)
    let base_config = None;

    // Validate and build the configuration
    let config = match cli.validate_and_build_config(base_config) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("Error: {}", err);
            process::exit(1);
        }
    };

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
        Commands::Package(pkg_cmd) => match &pkg_cmd.command {
            PackageSubcommands::Install { package_name } => {
                // Use the enhanced installer with progress display
                let installer = PackageInstaller::new(
                    fs,
                    runner,
                    config,
                    cli.verbose,
                    !cli.no_color,
                    true, // use_unicode
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
            PackageSubcommands::List => {
                let info_pb = progress_manager.create_progress_bar(
                    "list",
                    "Package listing not implemented yet",
                    selfie::progress_display::ProgressStyleType::Message,
                );
                info_pb.finish();
                0
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
            PackageSubcommands::Validate { package_name, .. } => {
                let info_pb = progress_manager.create_progress_bar(
                    "validate",
                    &format!(
                        "Package validation for '{}' not implemented yet",
                        package_name
                    ),
                    selfie::progress_display::ProgressStyleType::Message,
                );
                info_pb.finish();
                0
            }
        },
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
