// src/main.rs

use std::process;

use selfie::{
    adapters::{
        command::shell::ShellCommandRunner, config_loader, filesystem::RealFileSystem,
        progress::ProgressManager, user_interface::ClapCli,
    },
    ports::{
        application::{ApplicationCommandRouter, ArgumentParser},
        config_loader::ConfigLoader,
    },
    services::command::application::ApplicationCommandService,
};

#[tokio::main]
async fn main() -> Result<(), anyhow::Error> {
    // Set up file system and command runner
    let fs = RealFileSystem;

    let (app_config, args) = {
        // Parse the command line arguments
        let args = match ClapCli::parse_arguments() {
            Ok(args) => args,
            Err(err) => {
                ProgressManager::new(false, true).print_error(format!("Error: {}", err));
                process::exit(1);
            }
        };

        (
            config_loader::Yaml::new(&fs)
                .load_config(&args)?
                .apply_cli_args(&args),
            args,
        )
    };
    let cmd_service = {
        let runner = ShellCommandRunner::new("/bin/sh", app_config.command_timeout());

        // Create the command service to route and execute the command
        ApplicationCommandService::new(&fs, Box::new(runner), &app_config)
    };

    // Process the command and get an exit code
    match cmd_service.process_command(args).await {
        Ok(code) => process::exit(code),
        Err(err) => {
            // Create a progress manager for error formatting
            let progress_manager =
                ProgressManager::new(app_config.use_colors(), app_config.verbose());

            // Format and print the error
            progress_manager.print_error(format!("Error: {}", err));

            // // If we have detailed error information and verbose is enabled, print it
            // if app_config.verbose() {
            //     // Create context for the error
            //     let context =
            //         ErrorContext::default().with_message("Error occurred while processing command");
            //
            //     progress_manager.print_info(format!("Error context: {}", context));
            // }

            process::exit(1)
        }
    }
}
