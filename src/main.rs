// src/main.rs

use std::{process, time::Duration};

use selfie::{
    cli::{self, Cli, Commands, PackageSubcommands},
    command::ShellCommandRunner,
    filesystem::RealFileSystem,
    package_installer::PackageInstaller,
    progress::{ConsoleRenderer, ProgressReporter},
};

fn main() {
    // Parse command line arguments
    let cli = Cli::parse_args();

    // Configure renderer based on CLI options
    let renderer = Box::new(ConsoleRenderer::new(!cli.no_color, !cli.no_color));
    let reporter = ProgressReporter::new(renderer);

    // Create a base configuration (in a real app, this would be loaded from a file)
    let base_config = None;

    // Validate and build the configuration
    let config = match cli.validate_and_build_config(base_config) {
        Ok(config) => config,
        Err(err) => {
            eprintln!("{}", reporter.error(&err.to_string()));
            process::exit(1);
        }
    };

    // Set up file system and command runner
    let fs = RealFileSystem;
    let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(60));

    // Display command that will be executed
    let cmd_desc = cli::get_command_description(&cli);
    println!("{}", reporter.info(&cmd_desc));

    // Execute the command
    let result = match &cli.command {
        Commands::Package(pkg_cmd) => match &pkg_cmd.command {
            PackageSubcommands::Install { package_name } => {
                let installer =
                    PackageInstaller::new(fs, runner, config, reporter.clone(), cli.verbose);
                match installer.install_package(package_name) {
                    Ok(_) => 0,
                    Err(err) => {
                        eprintln!(
                            "{}",
                            reporter.error(&format!("Installation failed: {}", err))
                        );
                        1
                    }
                }
            }
            PackageSubcommands::List => {
                println!("{}", reporter.info("Package listing not implemented yet"));
                0
            }
            PackageSubcommands::Info { package_name } => {
                println!(
                    "{}",
                    reporter.info(&format!(
                        "Package info for '{}' not implemented yet",
                        package_name
                    ))
                );
                0
            }
            PackageSubcommands::Create { package_name } => {
                println!(
                    "{}",
                    reporter.info(&format!(
                        "Package creation for '{}' not implemented yet",
                        package_name
                    ))
                );
                0
            }
            PackageSubcommands::Validate { package_name, .. } => {
                println!(
                    "{}",
                    reporter.info(&format!(
                        "Package validation for '{}' not implemented yet",
                        package_name
                    ))
                );
                0
            }
        },
        Commands::Config(_cfg_cmd) => {
            println!("{}", reporter.info("Config commands not implemented yet"));
            0
        }
    };

    process::exit(result);
}
