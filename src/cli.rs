// src/cli.rs
// impl Cli {
//     /// Parse command line arguments into a Cli structure
//     pub fn parse_args() -> Self {
//         Cli::parse()
//     }
//
//     /// Validate CLI options and build a Config from them
//     pub fn validate_and_build_config(
//         &self,
//         base_config: Option<Config>,
//     ) -> Result<Config, CliError> {
//         // Start with either the provided base config or a default one
//         let mut config = base_config.unwrap_or_else(|| Config::new(String::new(), PathBuf::new()));
//
//         // Override with CLI options
//         if let Some(env) = &self.environment {
//             config = Config::new(env.clone(), config.package_directory.clone());
//         }
//
//         if let Some(package_dir) = &self.package_directory {
//             config = Config::new(config.environment.clone(), package_dir.clone());
//         }
//
//         // Validate final configuration using the full validation
//         config.validate().map_err(CliError::ConfigError)?;
//
//         Ok(config)
//     }
//
//     /// Build a Config with minimal validation (for commands that don't need a complete config)
//     pub fn build_minimal_config(&self, base_config: Option<Config>) -> Result<Config, CliError> {
//         // Start with either the provided base config or a default one
//         let mut config = base_config.unwrap_or_else(|| Config::new(String::new(), PathBuf::new()));
//
//         // Override with CLI options
//         if let Some(env) = &self.environment {
//             config = Config::new(env.clone(), config.package_directory.clone());
//         }
//
//         if let Some(package_dir) = &self.package_directory {
//             config = Config::new(config.environment.clone(), package_dir.clone());
//         }
//
//         // Only validate that the package directory is valid
//         config.validate_minimal().map_err(CliError::ConfigError)?;
//
//         Ok(config)
//     }
// }
