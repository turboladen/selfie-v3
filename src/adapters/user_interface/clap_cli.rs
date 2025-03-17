// src/adapters/cli/clap_adapter.rs
use std::path::PathBuf;

use clap::{Args, Parser, Subcommand};

use crate::{
    domain,
    ports::application::{ApplicationArguments, ArgumentParser},
};

/// Selfie - A package manager and dotfile manager
#[derive(Parser, Debug)]
#[clap(author, version, about, long_about = None)]
pub struct ClapCli {
    /// Override the environment from config
    #[clap(long, short = 'e', global = true)]
    pub(crate) environment: Option<String>,

    /// Override the package directory from config
    #[clap(long, short = 'p', global = true)]
    pub(crate) package_directory: Option<PathBuf>,

    /// Show detailed output
    #[clap(long, short = 'v', global = true)]
    pub(crate) verbose: bool,

    /// Disable colored output
    #[clap(long, global = true)]
    pub(crate) no_color: bool,

    /// Subcommand to execute
    #[clap(subcommand)]
    pub(crate) command: ClapCommands,
}

// Clap-specific command structure definitions here...
#[derive(Subcommand, Debug, Clone)]
pub(crate) enum ClapCommands {
    /// Package management commands
    Package(PackageCommands),

    /// Configuration management commands
    Config(ConfigCommands),
}

#[derive(Args, Debug, Clone)]
pub(crate) struct PackageCommands {
    #[clap(subcommand)]
    pub(crate) command: PackageSubcommands,
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum PackageSubcommands {
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
pub(crate) struct ConfigCommands {
    #[clap(subcommand)]
    pub(crate) command: ConfigSubcommands,
}

#[derive(Subcommand, Debug, Clone)]
pub(crate) enum ConfigSubcommands {
    /// Validate the selfie configuration
    Validate,
}

impl ArgumentParser for ClapCli {
    fn parse_arguments() -> Result<ApplicationArguments, anyhow::Error> {
        let clap_args = Self::parse();

        // Convert from Clap-specific format to application format
        Ok(ApplicationArguments::from(clap_args))
    }
}

impl From<ClapCli> for ApplicationArguments {
    fn from(value: ClapCli) -> Self {
        Self {
            environment: value.environment,
            package_directory: value.package_directory,
            verbose: value.verbose,
            no_color: value.no_color,
            command: domain::application::commands::ApplicationCommand::from(value.command),
        }
    }
}

impl From<ClapCommands> for domain::application::commands::ApplicationCommand {
    fn from(value: ClapCommands) -> Self {
        match value {
            ClapCommands::Package(package_commands) => Self::Package(
                domain::application::commands::PackageCommand::from(package_commands.command),
            ),
            ClapCommands::Config(config_commands) => Self::Config(
                domain::application::commands::ConfigCommand::from(config_commands.command),
            ),
        }
    }
}

impl From<PackageSubcommands> for domain::application::commands::PackageCommand {
    fn from(value: PackageSubcommands) -> Self {
        match value {
            PackageSubcommands::Install { package_name } => {
                domain::application::commands::PackageCommand::Install { package_name }
            }
            PackageSubcommands::List => domain::application::commands::PackageCommand::List,
            PackageSubcommands::Info { package_name } => {
                domain::application::commands::PackageCommand::Info { package_name }
            }
            PackageSubcommands::Create { package_name } => {
                domain::application::commands::PackageCommand::Create { package_name }
            }
            PackageSubcommands::Validate {
                package_name,
                package_path,
            } => domain::application::commands::PackageCommand::Validate {
                package_name,
                package_path,
            },
        }
    }
}

impl From<ConfigSubcommands> for domain::application::commands::ConfigCommand {
    fn from(value: ConfigSubcommands) -> Self {
        match value {
            ConfigSubcommands::Validate => domain::application::commands::ConfigCommand::Validate,
        }
    }
}
