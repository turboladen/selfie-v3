// src/adapters/config/yaml_config_loader.rs
use std::path::PathBuf;

use serde_yaml;

use crate::{
    domain::config::Config,
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
    //
    // /// Get the XDG config directory
    // fn get_xdg_config_dir(&self) -> Option<PathBuf> {
    //     env::var("XDG_CONFIG_HOME")
    //         .map(|path| PathBuf::from(path))
    //         .ok()
    // }
    //
    // /// Get the user's home directory
    // fn get_home_dir(&self) -> Option<PathBuf> {
    //     dirs::home_dir()
    // }
    //
    // /// Check if a path exists and is readable
    // fn is_readable_file(&self, path: &Path) -> bool {
    //     self.fs.path_exists(path)
    // }
}

impl<F: FileSystem> ConfigLoader for YamlConfigLoader<'_, F> {
    fn load_config(&self) -> Result<Config, ConfigLoadError> {
        let config_paths = self.find_config_paths();

        if config_paths.is_empty() {
            // No config file found, return default
            return Ok(self.default_config());
        }

        // Use the first config file found
        self.load_config_from_path(&config_paths[0])
    }

    fn load_config_from_path(&self, path: &PathBuf) -> Result<Config, ConfigLoadError> {
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
        let config: Config = serde_yaml::from_str(&content)
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
            let config_yaml = config_dir.join("selfie").join("config.yaml");
            let config_yml = config_dir.join("selfie").join("config.yml");

            if self.fs.path_exists(&config_yaml) {
                paths.push(config_yaml);
            }
            if self.fs.path_exists(&config_yml) {
                paths.push(config_yml);
            }
        }

        paths
    }

    fn default_config(&self) -> Config {
        // Create a minimal default configuration
        Config::new(String::new(), PathBuf::new())
    }
}

// src/adapters/config/yaml_config_loader_test.rs
#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::filesystem::MockFileSystem;
    use std::path::Path;

    fn setup_test_fs() -> MockFileSystem {
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

        fs
    }

    #[test]
    fn test_find_config_paths() {
        let mut fs = setup_test_fs();
        let config_dir = Path::new("/home/test/.config");
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
        let mut fs = setup_test_fs();
        let config_dir = Path::new("/home/test/.config");
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
        let config_dir = Path::new("/home/test/.config");
        fs.mock_config_dir(&config_dir);
        fs.mock_path_exists(config_dir, false);
        fs.mock_path_exists(config_dir.join("selfie").join("config.yaml"), false);
        fs.mock_path_exists(config_dir.join("selfie").join("config.yml"), false);

        let loader = YamlConfigLoader::new(&fs);

        // Should return default config
        let config = loader.load_config().unwrap();

        assert_eq!(config.environment, "");
        assert!(config.package_directory.as_os_str().is_empty());
    }
}
