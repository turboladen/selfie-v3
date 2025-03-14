// src/main.rs

use std::process;

use selfie::{
    adapters::{
        command::shell::ShellCommandRunner, config_loader, filesystem::RealFileSystem,
        progress::ProgressManager, user_interface::ClapCli,
    },
    domain::errors::ErrorContext,
    ports::{
        application::{ApplicationCommandRouter, ArgumentParser},
        config_loader::ConfigLoader,
    },
    services::application_command_service::ApplicationCommandService,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Set up file system and command runner
    let fs = RealFileSystem;

    // Parse the command line arguments
    let args = match ClapCli::parse_arguments() {
        Ok(args) => args,
        Err(err) => {
            let progress_manager = ProgressManager::new(false, true);
            progress_manager.print_error(format!("Error: {}", err));
            process::exit(1);
        }
    };

    let app_config = config_loader::Yaml::new(&fs)
        .load_config(&args)?
        .apply_cli_args(&args);

    let runner = ShellCommandRunner::new("/bin/sh", app_config.command_timeout());

    // Create the command service to route and execute the command
    let cmd_service = ApplicationCommandService::new(&fs, &runner, &app_config);

    // Create a progress manager for error formatting
    let progress_manager = ProgressManager::new(!args.no_color, args.verbose);

    // Process the command and get an exit code
    match cmd_service.process_command(args).await {
        Ok(code) => process::exit(code),
        Err(err) => {
            // Create context for the error
            let context =
                ErrorContext::default().with_message("Error occurred while processing command");

            // Format and print the error
            progress_manager.print_error(format!("Error: {}", err));

            // If we have detailed error information and verbose is enabled, print it
            if progress_manager.verbose() {
                progress_manager.print_info(format!("Error context: {}", context));
            }

            process::exit(1)
        }
    }
}
