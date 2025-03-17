// src/ports/application.rs
use std::path::PathBuf;

use crate::domain::application::commands::ApplicationCommand;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApplicationArguments {
    pub(crate) environment: Option<String>,
    pub(crate) package_directory: Option<PathBuf>,
    pub verbose: bool,
    pub no_color: bool,
    pub(crate) command: ApplicationCommand,
}

#[derive(Debug, Default)]
#[cfg(test)]
pub(crate) struct ApplicationArgumentsBuilder {
    environment: Option<String>,
    package_directory: Option<PathBuf>,
    verbose: bool,
    no_color: bool,
    command: ApplicationCommand,
}

#[cfg(test)]
impl ApplicationArgumentsBuilder {
    pub(crate) fn environment(mut self, environment: &str) -> Self {
        self.environment = Some(environment.to_string());
        self
    }

    pub(crate) fn package_directory<P>(mut self, package_directory: P) -> Self
    where
        PathBuf: From<P>,
    {
        self.package_directory = Some(package_directory.into());
        self
    }

    pub(crate) fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub(crate) fn no_color(mut self, no_color: bool) -> Self {
        self.no_color = no_color;
        self
    }

    pub(crate) fn command(mut self, command: ApplicationCommand) -> Self {
        self.command = command;
        self
    }

    pub(crate) fn build(self) -> ApplicationArguments {
        ApplicationArguments {
            environment: self.environment,
            package_directory: self.package_directory,
            verbose: self.verbose,
            no_color: self.no_color,
            command: self.command,
        }
    }
}

pub trait ArgumentParser {
    /// Parse arguments into the application's domain model
    fn parse_arguments() -> Result<ApplicationArguments, anyhow::Error>;
}

#[async_trait::async_trait]
pub trait ApplicationCommandRouter {
    /// Process an application command and return an exit code
    async fn process_command(&self, args: ApplicationArguments) -> Result<i32, anyhow::Error>;

    /// Get a human-readable description of a command
    fn get_command_description(&self, command: &ApplicationCommand) -> String;
}
