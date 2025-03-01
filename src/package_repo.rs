// src/package_repo.rs - Update list_packages implementation

use std::path::{Path, PathBuf};

use thiserror::Error;

use crate::{
    domain::package::{Package, PackageParseError},
    ports::filesystem::FileSystem,
};

#[derive(Error, Debug)]
pub enum PackageRepoError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Multiple packages found with name: {0}")]
    MultiplePackagesFound(String),

    #[error("Parse error: {0}")]
    ParseError(#[from] PackageParseError),

    #[error("IO error: {0}")]
    IoError(#[from] std::io::Error),

    #[error("Directory does not exist: {0}")]
    DirectoryNotFound(String),
}

pub struct PackageRepository<'a, F: FileSystem> {
    fs: &'a F,
    package_dir: PathBuf,
}

impl<'a, F: FileSystem> PackageRepository<'a, F> {
    pub fn new(fs: &'a F, package_dir: PathBuf) -> Self {
        Self { fs, package_dir }
    }

    /// Get a package by name
    pub fn get_package(&self, name: &str) -> Result<Package, PackageRepoError> {
        let package_files = self.find_package_files(name)?;

        if package_files.is_empty() {
            return Err(PackageRepoError::PackageNotFound(name.to_string()));
        }

        if package_files.len() > 1 {
            return Err(PackageRepoError::MultiplePackagesFound(name.to_string()));
        }

        let package_file = &package_files[0];
        let package = Package::from_file(&self.fs, package_file)?;

        Ok(package)
    }

    /// List all available packages in the package directory
    pub fn list_packages(&self) -> Result<Vec<Package>, PackageRepoError> {
        if !self.fs.path_exists(&self.package_dir) {
            return Err(PackageRepoError::DirectoryNotFound(
                self.package_dir.to_string_lossy().into_owned(),
            ));
        }

        // Get all YAML files in the directory
        let yaml_files = self.list_yaml_files(&self.package_dir)?;

        // Parse each file into a Package
        let mut packages = Vec::new();
        for path in yaml_files {
            match Package::from_file(self.fs, &path) {
                Ok(package) => packages.push(package),
                Err(err) => {
                    // Skip invalid files but log them if we had a proper logging system
                    if let Some(file_name) = path.file_name().and_then(|n| n.to_str()) {
                        eprintln!(
                            "Warning: Failed to parse package file '{}': {}",
                            file_name, err
                        );
                    }
                }
            }
        }

        Ok(packages)
    }

    // Find package files that match the given name
    pub fn find_package_files(&self, name: &str) -> Result<Vec<PathBuf>, PackageRepoError> {
        if !self.fs.path_exists(&self.package_dir) {
            return Err(PackageRepoError::DirectoryNotFound(
                self.package_dir.to_string_lossy().into_owned(),
            ));
        }

        // Look for both name.yaml and name.yml
        let yaml_path = self.package_dir.join(format!("{}.yaml", name));
        let yml_path = self.package_dir.join(format!("{}.yml", name));

        let mut result = Vec::new();
        if self.fs.path_exists(&yaml_path) {
            result.push(yaml_path);
        }
        if self.fs.path_exists(&yml_path) {
            result.push(yml_path);
        }

        Ok(result)
    }

    // List all YAML files in a directory
    fn list_yaml_files(&self, dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
        let entries = self
            .fs
            .list_directory(dir)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e.to_string()))?;

        let yaml_files: Vec<PathBuf> = entries
            .into_iter()
            .filter(|path| {
                if let Some(ext) = path.extension() {
                    let ext_str = ext.to_string_lossy().to_lowercase();
                    ext_str == "yaml" || ext_str == "yml"
                } else {
                    false
                }
            })
            .collect();

        Ok(yaml_files)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::mock::MockFileSystem;

    #[test]
    fn test_get_package_success() {
        let fs = MockFileSystem::default();
        let package_dir = PathBuf::from("/test/packages");

        // Create mock package file
        let package_path = package_dir.join("ripgrep.yaml");
        let yaml = r#"
            name: ripgrep
            version: 0.1.0
            environments:
              mac:
                install: brew install ripgrep
        "#;

        fs.add_file(&package_path, yaml);
        fs.add_existing_path(&package_dir);

        let repo = PackageRepository::new(&fs, package_dir);
        let package = repo.get_package("ripgrep").unwrap();

        assert_eq!(package.name, "ripgrep");
        assert_eq!(package.version, "0.1.0");
        assert_eq!(package.environments.len(), 1);
    }

    #[test]
    fn test_get_package_not_found() {
        let fs = MockFileSystem::default();
        let package_dir = PathBuf::from("/test/packages");

        fs.add_existing_path(&package_dir);

        let repo = PackageRepository::new(&fs, package_dir);
        let result = repo.get_package("nonexistent");

        assert!(matches!(result, Err(PackageRepoError::PackageNotFound(_))));
    }

    #[test]
    fn test_get_package_directory_not_found() {
        let fs = MockFileSystem::default();
        let package_dir = PathBuf::from("/test/nonexistent");

        let repo = PackageRepository::new(&fs, package_dir);
        let result = repo.get_package("ripgrep");

        assert!(matches!(
            result,
            Err(PackageRepoError::DirectoryNotFound(_))
        ));
    }

    #[test]
    fn test_get_package_multiple_found() {
        let fs = MockFileSystem::default();
        let package_dir = PathBuf::from("/test/packages");

        // Create multiple mock package files with the same name
        let yaml_path = package_dir.join("ripgrep.yaml");
        let yml_path = package_dir.join("ripgrep.yml");

        let yaml = r#"
            name: ripgrep
            version: 0.1.0
            environments:
              mac:
                install: brew install ripgrep
        "#;

        fs.add_file(&yaml_path, yaml);
        fs.add_file(&yml_path, yaml);
        fs.add_existing_path(&package_dir);

        let repo = PackageRepository::new(&fs, package_dir);
        let result = repo.get_package("ripgrep");

        assert!(matches!(
            result,
            Err(PackageRepoError::MultiplePackagesFound(_))
        ));
    }

    #[test]
    fn test_find_package_files() {
        let fs = MockFileSystem::default();
        let package_dir = PathBuf::from("/test/packages");

        // Create mock package files
        let yaml_path = package_dir.join("ripgrep.yaml");
        let yml_path = package_dir.join("other.yml");

        fs.add_file(&yaml_path, "dummy content");
        fs.add_file(&yml_path, "dummy content");
        fs.add_existing_path(&package_dir);

        let repo = PackageRepository::new(&fs, package_dir);

        // Should find ripgrep.yaml
        let files = repo.find_package_files("ripgrep").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], yaml_path);

        // Should find other.yml
        let files = repo.find_package_files("other").unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0], yml_path);

        // Should not find nonexistent
        let files = repo.find_package_files("nonexistent").unwrap();
        assert_eq!(files.len(), 0);
    }

    #[test]
    fn test_list_packages() {
        let fs = MockFileSystem::default();
        let package_dir = PathBuf::from("/test/packages");
        fs.add_existing_path(&package_dir);

        // Add valid package files
        let package1 = r#"
            name: ripgrep
            version: 1.0.0
            environments:
              test-env:
                install: brew install ripgrep
        "#;

        let package2 = r#"
            name: fzf
            version: 0.2.0
            environments:
              other-env:
                install: brew install fzf
        "#;

        fs.add_file(&package_dir.join("ripgrep.yaml"), package1);
        fs.add_file(&package_dir.join("fzf.yml"), package2);

        // Also add an invalid package file
        fs.add_file(&package_dir.join("invalid.yaml"), "not valid yaml: :");

        let repo = PackageRepository::new(&fs, package_dir);
        let packages = repo.list_packages().unwrap();

        // Should find both valid packages
        assert_eq!(packages.len(), 2);

        // Check package details
        let ripgrep = packages.iter().find(|p| p.name == "ripgrep").unwrap();
        let fzf = packages.iter().find(|p| p.name == "fzf").unwrap();

        assert_eq!(ripgrep.version, "1.0.0");
        assert!(ripgrep.environments.contains_key("test-env"));

        assert_eq!(fzf.version, "0.2.0");
        assert!(fzf.environments.contains_key("other-env"));
    }

    #[test]
    fn test_list_yaml_files() {
        let fs = MockFileSystem::default();
        let dir = PathBuf::from("/test/dir");
        fs.add_existing_path(&dir);

        // Add various file types
        fs.add_file(&dir.join("file1.yaml"), "content");
        fs.add_file(&dir.join("file2.yml"), "content");
        fs.add_file(&dir.join("file3.txt"), "content");
        fs.add_file(&dir.join("file4.YAML"), "content"); // Test case insensitivity
        fs.add_file(&dir.join("file5.YML"), "content"); // Test case insensitivity

        let repo = PackageRepository::new(&fs, PathBuf::from("/dummy")); // Path doesn't matter here
        let yaml_files = repo.list_yaml_files(&dir).unwrap();

        // Should find all yaml/yml files regardless of case
        assert_eq!(yaml_files.len(), 4);

        // Check each expected file is found
        assert!(yaml_files.contains(&dir.join("file1.yaml")));
        assert!(yaml_files.contains(&dir.join("file2.yml")));
        assert!(yaml_files.contains(&dir.join("file4.YAML")));
        assert!(yaml_files.contains(&dir.join("file5.YML")));

        // Check that non-yaml file is not included
        assert!(!yaml_files.contains(&dir.join("file3.txt")));
    }
}
