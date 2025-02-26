// src/package_repo.rs

use std::path::{Path, PathBuf};
use thiserror::Error;

use crate::filesystem::FileSystem;
use crate::package::{PackageNode, PackageParseError};

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
    pub fn get_package(&self, name: &str) -> Result<PackageNode, PackageRepoError> {
        let package_files = self.find_package_files(name)?;

        if package_files.is_empty() {
            return Err(PackageRepoError::PackageNotFound(name.to_string()));
        }

        if package_files.len() > 1 {
            return Err(PackageRepoError::MultiplePackagesFound(name.to_string()));
        }

        let package_file = &package_files[0];
        let package = PackageNode::from_file(&self.fs, package_file)?;

        Ok(package)
    }

    /// List all available packages in the package directory
    pub fn list_packages(&self) -> Result<Vec<PackageNode>, PackageRepoError> {
        if !self.fs.path_exists(&self.package_dir) {
            return Err(PackageRepoError::DirectoryNotFound(
                self.package_dir.to_string_lossy().into_owned(),
            ));
        }

        // This would need to be implemented in the real FileSystem implementation
        // Here we'll just stub it and use it in tests
        self.list_yaml_files(&self.package_dir)
            .map_err(PackageRepoError::IoError)?
            .into_iter()
            .filter_map(|path| {
                match PackageNode::from_file(&self.fs, &path) {
                    Ok(package) => Some(Ok(package)),
                    Err(_) => None, // Skip invalid files
                }
            })
            .collect()
    }

    // Find package files that match the given name
    fn find_package_files(&self, name: &str) -> Result<Vec<PathBuf>, PackageRepoError> {
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
    // This would need to be properly implemented in the FileSystem trait
    fn list_yaml_files(&self, _dir: &Path) -> Result<Vec<PathBuf>, std::io::Error> {
        // For now, we'll implement a simple stub that we can use in tests
        // In a real implementation, this would be part of the FileSystem trait

        Ok(Vec::new()) // Stub that returns empty vector
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
}
