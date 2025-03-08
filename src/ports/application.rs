// src/ports/application.rs
use std::path::PathBuf;

use thiserror::Error;

use crate::domain::{
    application::commands::ApplicationCommand,
    config::{ConfigValidationError, ConfigValidationErrors},
};

#[derive(Debug, Clone)]
pub struct ApplicationArguments {
    pub environment: Option<String>,
    pub package_directory: Option<PathBuf>,
    pub verbose: bool,
    pub no_color: bool,
    pub command: ApplicationCommand,
}

#[derive(Error, Debug)]
pub enum ApplicationError {
    #[error("Invalid application arguments: {0}")]
    InvalidArguments(String),

    #[error("Configuration errors: {0}")]
    ConfigErrors(#[from] ConfigValidationErrors),

    #[error("Configuration error: {0}")]
    ConfigError(#[from] ConfigValidationError),

    #[error("Command execution error: {0}")]
    ExecutionError(String),
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
