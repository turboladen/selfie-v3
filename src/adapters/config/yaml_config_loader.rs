// src/adapters/config/yaml_config_loader.rs
use std::path::{Path, PathBuf};

use serde_yaml;

use crate::{
    domain::config::FileConfig,
    ports::{
        config_loader::{ConfigLoadError, ConfigLoader},
        filesystem::FileSystem,
    },
};

pub struct YamlConfigLoader<'a, F: FileSystem> {
    fs: &'a F,
}

impl<'a, F: FileSystem> YamlConfigLoader<'a, F> {
    pub fn new(fs: &'a F) -> Self {
        Self { fs }
    }
}

impl<F: FileSystem> ConfigLoader for YamlConfigLoader<'_, F> {
    fn load_config(&self) -> Result<FileConfig, ConfigLoadError> {
        let config_paths = self.find_config_paths();

        if config_paths.is_empty() {
            // No config file found, return error
            return Err(ConfigLoadError::NotFound);
        }

        // Use the first config file found
        self.load_config_from_path(&config_paths[0])
    }

    fn load_config_from_path(&self, path: &Path) -> Result<FileConfig, ConfigLoadError> {
        if !self.fs.path_exists(path) {
            return Err(ConfigLoadError::ReadError(format!(
                "Can't read config file: {}",
                path.display()
            )));
        }

        // Read the file
        let content = self
            .fs
            .read_file(path)
            .map_err(|e| ConfigLoadError::ReadError(e.to_string()))?;

        // Parse the YAML content
        let config: FileConfig = serde_yaml::from_str(&content)
            .map_err(|e| ConfigLoadError::ParseError(e.to_string()))?;

        // Validate the config
        config
            .validate()
            .map_err(|e| ConfigLoadError::ValidationError(e.to_string()))?;

        Ok(config)
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

    fn default_config(&self) -> FileConfig {
        // Create a minimal default configuration
        FileConfig::new(String::new(), PathBuf::new())
    }
}

// src/adapters/config/yaml_config_loader_test.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::filesystem::MockFileSystem;
    use std::{
        num::{NonZeroU64, NonZeroUsize},
        path::Path,
    };

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

        let loader = YamlConfigLoader::new(&fs);

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
        fs.mock_path_exists(config_dir.join("selfie").join("config.yaml"), true);

        let loader = YamlConfigLoader::new(&fs);

        let config = loader.load_config().unwrap();

        // Check the loaded values
        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, Path::new("/test/packages"));
    }

    #[test]
    fn test_load_config_not_found() {
        let mut fs = MockFileSystem::default(); // Empty file system
        let config_dir = Path::new("/home/test/.config/selfie");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(config_dir, true);
        fs.mock_path_exists(config_dir.join("config.yaml"), false);
        fs.mock_path_exists(config_dir.join("config.yml"), false);

        let loader = YamlConfigLoader::new(&fs);

        // Should return error
        let result = loader.load_config();
        assert!(matches!(result, Err(ConfigLoadError::NotFound)));
    }

    #[test]
    fn test_load_config_with_extended_settings() {
        let mut fs = MockFileSystem::default();
        let config_dir = Path::new("/home/test/.config");

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

        let config_path = config_dir.join("selfie").join("config.yaml");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(&config_path, true);
        fs.mock_read_file(&config_path, config_yaml);

        let loader = YamlConfigLoader::new(&fs);
        let config = loader.load_config_from_path(&config_path).unwrap();

        // Check basic settings
        assert_eq!(config.environment, "test-env");
        assert_eq!(config.package_directory, Path::new("/test/packages"));

        // Check extended settings
        assert_eq!(config.command_timeout, NonZeroU64::new(120));
        assert_eq!(config.stop_on_error, Some(false));
        assert_eq!(config.max_parallel_installations, NonZeroUsize::new(8));

        // Check logging settings
        let logging = config.logging.unwrap();
        assert!(logging.enabled);
        assert_eq!(logging.directory, Path::new("/test/logs"));
        assert_eq!(logging.max_files, NonZeroUsize::new(5).unwrap());
        assert_eq!(logging.max_size, NonZeroUsize::new(20).unwrap());
    }
}
