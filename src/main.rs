// src/main.rs

use std::{process, time::Duration};

use selfie::{
    adapters::{
        cli::clap_adapter::ClapArguments, command::shell::ShellCommandRunner,
        config::yaml_config_loader::YamlConfigLoader, filesystem::RealFileSystem,
    },
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

    // Process the command and get an exit code
    let exit_code = match cmd_service.process_command(args) {
        Ok(code) => code,
        Err(err) => {
            eprintln!("Error: {}", err);
            1
        }
    };

    process::exit(exit_code);
}
