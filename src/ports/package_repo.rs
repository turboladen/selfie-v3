// src/ports/package_repo.rs
use std::path::PathBuf;
use thiserror::Error;

use crate::domain::package::{Package, PackageParseError};

#[derive(Error, Debug)]
pub(crate) enum PackageRepoError {
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

/// Port for package repository operations
#[cfg_attr(test, mockall::automock)]
pub(crate) trait PackageRepository {
    /// Get a package by name
    fn get_package(&self, name: &str) -> Result<Package, PackageRepoError>;

    /// List all available packages in the package directory
    fn list_packages(&self) -> Result<Vec<Package>, PackageRepoError>;

    /// Find package files that match the given name
    fn find_package_files(&self, name: &str) -> Result<Vec<PathBuf>, PackageRepoError>;
}

#[cfg(test)]
impl MockPackageRepository {
    pub(crate) fn mock_get_package_ok(&mut self, name: &str, result: Package) {
        let name = name.to_string();

        self.expect_get_package()
            .with(mockall::predicate::eq(name))
            .returning(move |_| Ok(result.clone()));
    }

    pub(crate) fn mock_get_package_err(&mut self, name: &str, result: PackageRepoError) {
        let name = name.to_string();

        self.expect_get_package()
            .with(mockall::predicate::eq(name))
            .return_once(move |_| Err(result));
    }
}
