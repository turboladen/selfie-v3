// src/cli.rs

use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};
use thiserror::Error;

use crate::config::{Config, ConfigValidationError};

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

        // Validate final configuration
        config.validate().map_err(CliError::ConfigError)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigBuilder;
    use clap::CommandFactory;

    #[test]
    fn test_cli_verify() {
        Cli::command().debug_assert();
    }

    #[test]
    fn test_get_command_description() {
        let cli = Cli {
            environment: None,
            package_directory: None,
            verbose: false,
            no_color: false,
            command: Commands::Package(PackageCommands {
                command: PackageSubcommands::Install {
                    package_name: "ripgrep".to_string(),
                },
            }),
        };

        assert_eq!(get_command_description(&cli), "Install package 'ripgrep'");
    }

    #[test]
    fn test_validate_and_build_config_with_base() {
        let base_config = ConfigBuilder::default()
            .environment("base-env")
            .package_directory("/base/path")
            .build();

        let cli = Cli {
            environment: Some("cli-env".to_string()),
            package_directory: None,
            verbose: false,
            no_color: false,
            command: Commands::Package(PackageCommands {
                command: PackageSubcommands::List,
            }),
        };

        let config = cli.validate_and_build_config(Some(base_config)).unwrap();
        assert_eq!(config.environment, "cli-env");
        assert_eq!(config.package_directory, PathBuf::from("/base/path"));
    }

    #[test]
    fn test_validate_and_build_config_no_base() {
        let cli = Cli {
            environment: Some("cli-env".to_string()),
            package_directory: Some(PathBuf::from("/cli/path")),
            verbose: false,
            no_color: false,
            command: Commands::Package(PackageCommands {
                command: PackageSubcommands::List,
            }),
        };

        let config = cli.validate_and_build_config(None).unwrap();
        assert_eq!(config.environment, "cli-env");
        assert_eq!(config.package_directory, PathBuf::from("/cli/path"));
    }

    #[test]
    fn test_validate_and_build_config_invalid() {
        let cli = Cli {
            environment: None, // Missing required field
            package_directory: Some(PathBuf::from("/cli/path")),
            verbose: false,
            no_color: false,
            command: Commands::Package(PackageCommands {
                command: PackageSubcommands::List,
            }),
        };

        let result = cli.validate_and_build_config(None);
        assert!(result.is_err());
        if let Err(CliError::ConfigError(ConfigValidationError::EmptyField(field))) = result {
            assert_eq!(field, "environment");
        } else {
            panic!("Unexpected error type");
        }
    }
}
