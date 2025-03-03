// src/cli.rs

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use thiserror::Error;

use crate::domain::config::{Config, ConfigValidationError};

#[derive(Debug, Error)]
pub enum CliError {
    #[error("Invalid command line arguments: {0}")]
    InvalidArguments(String),

    #[error("Configuration error: {0}")]
    ConfigError(#[from] ConfigValidationError),
}

/// Selfie - A package manager and dotfile manager
#[derive(Parser, Debug, Clone)]
#[clap(author, version, about, long_about = None)]
pub struct Cli {
    /// Override the environment from config
    #[clap(long, short = 'e', global = true)]
    pub environment: Option<String>,

    /// Override the package directory from config
    #[clap(long, short = 'p', global = true)]
    pub package_directory: Option<PathBuf>,

    /// Show detailed output
    #[clap(long, short = 'v', global = true)]
    pub verbose: bool,

    /// Disable colored output
    #[clap(long, global = true)]
    pub no_color: bool,

    /// Subcommand to execute
    #[clap(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum Commands {
    /// Package management commands
    Package(PackageCommands),

    /// Configuration management commands
    Config(ConfigCommands),
}

#[derive(Args, Debug, Clone)]
pub struct PackageCommands {
    #[clap(subcommand)]
    pub command: PackageSubcommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum PackageSubcommands {
    /// Install a package
    Install {
        /// Name of the package to install
        package_name: String,
    },

    /// List available packages
    List,

    /// Show information about a package
    Info {
        /// Name of the package to get information about
        package_name: String,
    },

    /// Create a new package
    Create {
        /// Name of the package to create
        package_name: String,
    },

    /// Validate a package
    Validate {
        /// Name of the package to validate
        package_name: String,

        /// Package file path (optional)
        #[clap(long)]
        package_path: Option<PathBuf>,
    },
}

#[derive(Args, Debug, Clone)]
pub struct ConfigCommands {
    #[clap(subcommand)]
    pub command: ConfigSubcommands,
}

#[derive(Subcommand, Debug, Clone)]
pub enum ConfigSubcommands {
    /// Validate the selfie configuration
    Validate,
}

impl Cli {
    /// Parse command line arguments into a Cli structure
    pub fn parse_args() -> Self {
        Cli::parse()
    }

    /// Validate CLI options and build a Config from them
    pub fn validate_and_build_config(
        &self,
        base_config: Option<Config>,
    ) -> Result<Config, CliError> {
        // Start with either the provided base config or a default one
        let mut config = base_config.unwrap_or_else(|| Config::new(String::new(), PathBuf::new()));

        // Override with CLI options
        if let Some(env) = &self.environment {
            config = Config::new(env.clone(), config.package_directory.clone());
        }

        if let Some(package_dir) = &self.package_directory {
            config = Config::new(config.environment.clone(), package_dir.clone());
        }

        // Validate final configuration using the full validation
        config.validate().map_err(CliError::ConfigError)?;

        Ok(config)
    }

    /// Build a Config with minimal validation (for commands that don't need a complete config)
    pub fn build_minimal_config(&self, base_config: Option<Config>) -> Result<Config, CliError> {
        // Start with either the provided base config or a default one
        let mut config = base_config.unwrap_or_else(|| Config::new(String::new(), PathBuf::new()));

        // Override with CLI options
        if let Some(env) = &self.environment {
            config = Config::new(env.clone(), config.package_directory.clone());
        }

        if let Some(package_dir) = &self.package_directory {
            config = Config::new(config.environment.clone(), package_dir.clone());
        }

        // Only validate that the package directory is valid
        config.validate_minimal().map_err(CliError::ConfigError)?;

        Ok(config)
    }
}

pub fn get_command_description(cli: &Cli) -> String {
    match &cli.command {
        Commands::Package(pkg_cmd) => match &pkg_cmd.command {
            PackageSubcommands::Install { package_name } => {
                format!("Install package '{}'", package_name)
            }
            PackageSubcommands::List => "List available packages".to_string(),
            PackageSubcommands::Info { package_name } => {
                format!("Show information about package '{}'", package_name)
            }
            PackageSubcommands::Create { package_name } => {
                format!("Create package '{}'", package_name)
            }
            PackageSubcommands::Validate { package_name, .. } => {
                format!("Validate package '{}'", package_name)
            }
        },
        Commands::Config(cfg_cmd) => match &cfg_cmd.command {
            ConfigSubcommands::Validate => "Validate configuration".to_string(),
        },
    }
}
