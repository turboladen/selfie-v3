// src/ports/application.rs
use std::path::PathBuf;

use thiserror::Error;

use crate::domain::{application::commands::ApplicationCommand, config::ConfigValidationError};

use super::config_loader::ConfigLoadError;

#[derive(Debug, Clone, Default, PartialEq)]
pub struct ApplicationArguments {
    pub environment: Option<String>,
    pub package_directory: Option<PathBuf>,
    pub verbose: bool,
    pub no_color: bool,
    pub command: ApplicationCommand,
}

#[derive(Debug, Default)]
pub struct ApplicationArgumentsBuilder {
    environment: Option<String>,
    package_directory: Option<PathBuf>,
    verbose: bool,
    no_color: bool,
    command: ApplicationCommand,
}

impl ApplicationArgumentsBuilder {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn environment(mut self, environment: &str) -> Self {
        self.environment = Some(environment.to_string());
        self
    }

    pub fn package_directory<P>(mut self, package_directory: P) -> Self
    where
        PathBuf: From<P>,
    {
        self.package_directory = Some(package_directory.into());
        self
    }

    pub fn verbose(mut self, verbose: bool) -> Self {
        self.verbose = verbose;
        self
    }

    pub fn no_color(mut self, no_color: bool) -> Self {
        self.no_color = no_color;
        self
    }

    pub fn command(mut self, command: ApplicationCommand) -> Self {
        self.command = command;
        self
    }

    pub fn build(self) -> ApplicationArguments {
        ApplicationArguments {
            environment: self.environment,
            package_directory: self.package_directory,
            verbose: self.verbose,
            no_color: self.no_color,
            command: self.command,
        }
    }
}

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("Invalid application arguments: {0}")]
    InvalidArguments(String),

    #[error("Configuration error: {0}")]
    ConfigError(#[from] ConfigValidationError),

    #[error("Command execution error: {0}")]
    ExecutionError(String),

    #[error(transparent)]
    ConfigLoadError(#[from] ConfigLoadError),
}

pub trait ArgumentParser {
    /// Parse arguments into the application's domain model
    fn parse_arguments() -> Result<ApplicationArguments, ApplicationError>;
}

pub trait ApplicationCommandRouter {
    /// Process an application command and return an exit code
    fn process_command(&self, args: ApplicationArguments) -> Result<i32, ApplicationError>;

    /// Get a human-readable description of a command
    fn get_command_description(&self, command: &ApplicationCommand) -> String;
}
