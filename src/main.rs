// src/main.rs

use std::{process, time::Duration};

use selfie::{
    adapters::{
        cli::clap_adapter::ClapArguments, command::shell::ShellCommandRunner,
        config::yaml_config_loader::YamlConfigLoader, filesystem::RealFileSystem,
        progress::ProgressManager,
    },
    domain::errors::ErrorContext,
    ports::application::{ApplicationCommandRouter, ArgumentParser},
    services::application_command_service::ApplicationCommandService,
};

fn main() {
    // Set up file system and command runner
    let fs = RealFileSystem;
    let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(60));
    let config_loader = YamlConfigLoader::new(&fs);

    // Parse the command line arguments
    let args = match ClapArguments::parse_arguments() {
        Ok(args) => args,
        Err(err) => {
            eprintln!("Error: {}", err);
            process::exit(1);
        }
    };

    // Create the command service to route and execute the command
    let cmd_service = ApplicationCommandService::new(&fs, &runner, &config_loader);

    // Create a progress manager for error formatting
    let progress_manager = ProgressManager::new(!args.no_color, args.verbose);

    // Process the command and get an exit code
    let exit_code = match cmd_service.process_command(args) {
        Ok(code) => code,
        Err(err) => {
            // Create context for the error
            let context =
                ErrorContext::new().with_message("Error occurred while processing command");

            // Format and print the error
            eprintln!("Error: {}", err);

            // If we have detailed error information and verbose is enabled, print it
            if progress_manager.verbose() {
                eprintln!("Error context: {}", context);
            }

            1
        }
    };

    process::exit(exit_code);
}
