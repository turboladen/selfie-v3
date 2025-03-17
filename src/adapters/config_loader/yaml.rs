// src/adapters/config/yaml_config_loader.rs
use std::path::PathBuf;

use config::FileFormat;

use crate::{
    domain::config::AppConfig,
    ports::{
        application::ApplicationArguments,
        config_loader::{ConfigLoadError, ConfigLoader},
        filesystem::FileSystem,
    },
};

pub struct Yaml<'a> {
    fs: &'a dyn FileSystem,
}

impl<'a> Yaml<'a> {
    pub fn new(fs: &'a dyn FileSystem) -> Self {
        Self { fs }
    }
}

impl ConfigLoader for Yaml<'_> {
    fn load_config(&self, app_args: &ApplicationArguments) -> Result<AppConfig, ConfigLoadError> {
        let config_paths = self.find_config_paths();

        if config_paths.is_empty() {
            // No config file found, return error
            return Err(ConfigLoadError::NotFound);
        }

        // Start with default configuration
        let mut builder = config::Config::builder();

        // Add default values
        builder = builder.set_default("verbose", false).unwrap();

        // builder = builder.add_source(config::File::from(config_paths[0].as_path()));
        let config_path = &config_paths[0];
        let file_contents = self
            .fs
            .read_file(config_path)
            .map_err(|e| ConfigLoadError::ReadError(e.to_string()))?;
        builder = builder.add_source(config::File::from_str(&file_contents, FileFormat::Yaml));

        // Add CLI overrides
        if let Some(environment) = app_args.environment.as_ref() {
            builder = builder.set_override("environment", environment.clone())?;
        }

        if let Some(package_directory) = app_args.package_directory.as_ref() {
            builder = builder.set_override(
                "package_directory",
                package_directory.clone().to_string_lossy().to_string(),
            )?;
        }

        builder = builder.set_override("verbose", app_args.verbose)?;
        builder = builder.set_override("use_colors", !app_args.no_color)?;

        // Build the config
        let config = builder.build()?;

        // Convert to our type
        let mut app_config: AppConfig = config.try_deserialize()?;

        // Special handling for package_directory ~ expansion
        if let Ok(expanded) = self.fs.expand_path(app_config.package_directory()) {
            app_config.package_directory = expanded;
        }

        // If logging is enabled but no directory specified, use default
        if app_config.logging.enabled && app_config.logging.directory.is_none() {
            return Err(ConfigLoadError::ValidationError(
                "Logging is enabled, but logging directory not set".to_string(),
            ));
        }

        Ok(app_config)
    }

    fn find_config_paths(&self) -> Vec<PathBuf> {
        let mut paths = Vec::new();

        if let Ok(config_dir) = self.fs.config_dir() {
            let config_yaml = config_dir.join("config.yaml");
            let config_yml = config_dir.join("config.yml");

            if self.fs.path_exists(&config_yaml) {
                paths.push(config_yaml);
            }
            if self.fs.path_exists(&config_yml) {
                paths.push(config_yml);
            }
        }

        paths
    }

    fn default_config(&self) -> AppConfig {
        // Create a minimal default configuration
        AppConfig::new(String::new(), PathBuf::new())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::{application::ApplicationArgumentsBuilder, filesystem::MockFileSystem};
    use std::path::Path;

    fn setup_test_fs() -> (MockFileSystem, PathBuf) {
        let mut fs = MockFileSystem::default();

        // Set up mock HOME environment for test
        let home_dir = Path::new("/home/test");

        // Create .config/selfie/config.yaml
        let config_yaml = r#"
            environment: "test-env"
            package_directory: "/test/packages"
        "#;

        let config_dir = home_dir.join(".config").join("selfie");
        let config_path = config_dir.join("config.yaml");

        fs.mock_path_exists(&config_path, true);
        fs.mock_path_exists(&config_dir.join("config.yml"), false);
        fs.mock_read_file(config_path, config_yaml);

        (fs, home_dir.into())
    }

    #[test]
    fn test_find_config_paths() {
        let (mut fs, home_dir) = setup_test_fs();
        let config_dir = home_dir.join(".config").join("selfie");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(config_dir.join("selfie").join("config.yaml"), true);

        let loader = Yaml::new(&fs);

        let paths = loader.find_config_paths();

        // Should find at least the one we set up
        assert!(!paths.is_empty());
        assert!(paths.iter().any(|p| p.ends_with("config.yaml")));
    }

    #[test]
    fn test_load_config() {
        let (mut fs, home_dir) = setup_test_fs();
        let config_dir = home_dir.join(".config").join("selfie");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(config_dir.join("config.yaml"), true);

        let package_dir = Path::new("/test/packages");
        fs.mock_path_exists(&package_dir, true);
        fs.mock_expand_path(&package_dir, &package_dir);

        let loader = Yaml::new(&fs);
        let args = ApplicationArgumentsBuilder::default()
            .environment("test-env")
            .package_directory(package_dir)
            .build();

        let config = loader.load_config(&args).unwrap();

        // Check the loaded values
        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, package_dir);
    }

    #[test]
    fn test_load_config_not_found() {
        let mut fs = MockFileSystem::default(); // Empty file system
        let config_dir = Path::new("/home/test/.config/selfie");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(config_dir, true);
        fs.mock_path_exists(config_dir.join("config.yaml"), false);
        fs.mock_path_exists(config_dir.join("config.yml"), false);

        let loader = Yaml::new(&fs);

        // Should return error
        let result = loader.load_config(&ApplicationArguments::default());
        assert!(matches!(result, Err(ConfigLoadError::NotFound)));
    }

    #[test]
    fn test_load_config_with_extended_settings() {
        let mut fs = MockFileSystem::default();
        let config_dir = Path::new("/home/test/.config/selfie");

        // Config with extended settings
        let config_yaml = r#"
            environment: "test-env"
            package_directory: "/test/packages"
            command_timeout: 120
            stop_on_error: false
            max_parallel_installations: 8
            logging:
              enabled: true
              directory: "/test/logs"
              max_files: 5
              max_size: 20
        "#;

        let config_path = config_dir.join("config.yaml");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(&config_path, true);
        fs.mock_path_exists(&config_dir.join("config.yml"), false);
        fs.mock_read_file(&config_path, config_yaml);

        fs.mock_expand_path("/test/packages", "/test/packages");

        let loader = Yaml::new(&fs);
        let config = loader
            .load_config(&ApplicationArguments::default())
            .unwrap();

        // Check basic settings
        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, Path::new("/test/packages"));

        // Check extended settings
        assert_eq!(config.command_timeout, 120.try_into().unwrap());
        assert!(!config.stop_on_error);
        assert_eq!(config.max_parallel_installations, 8.try_into().unwrap());

        // Check logging settings
        let logging = &config.logging;
        assert!(logging.enabled);
        assert_eq!(logging.directory.as_ref().unwrap(), Path::new("/test/logs"));
        assert_eq!(logging.max_files, 5.try_into().unwrap());
        assert_eq!(logging.max_size, 20.try_into().unwrap());
    }
}
