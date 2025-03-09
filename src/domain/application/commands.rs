use std::path::PathBuf;

#[derive(Debug, Clone, PartialEq)]
// pub enum Commands {
pub enum ApplicationCommand {
    /// Package management commands
    Package(PackageCommand),

    /// Configuration management commands
    Config(ConfigCommand),
}

impl Default for ApplicationCommand {
    fn default() -> Self {
        Self::Package(PackageCommand::List)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum PackageCommand {
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
        package_path: Option<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq)]
pub enum ConfigCommand {
    /// Validate the selfie configuration
    Validate,
}
