// src/ports/filesystem.rs
// File system port (interface)

use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Errors that can occur during file system operations
#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),

    #[error("Permission denied: {0}")]
    PermissionDenied(String),
}

/// Port for file system operations
#[mockall::automock]
pub trait FileSystem {
    /// Read a file and return its contents as a string
    fn read_file(&self, path: &Path) -> Result<String, FileSystemError>;

    /// Check if a path exists
    fn path_exists(&self, path: &Path) -> bool;

    /// Expand a path (e.g., expand ~ to home directory)
    fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError>;

    /// List the contents of a directory
    fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError>;

    /// Get the canonical path
    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FileSystemError>;
}

// Implement FileSystem for references to implement FileSystem
impl<T: FileSystem + ?Sized> FileSystem for &T {
    fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
        (*self).read_file(path)
    }

    fn path_exists(&self, path: &Path) -> bool {
        (*self).path_exists(path)
    }

    fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
        (*self).expand_path(path)
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
        (*self).list_directory(path)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
        (*self).canonicalize(path)
    }
}

// Helper functions to configure the mock filesystem
pub trait MockFileSystemExt {
    fn add_file(&mut self, path: &Path, content: &str);
    fn add_existing_path(&mut self, path: &Path);
}

impl MockFileSystemExt for MockFileSystem {
    fn add_file(&mut self, path: &Path, content: &str) {
        let path_buf = path.to_path_buf();
        let content_string = content.to_string();

        // Set up read_file to return the content for this path
        self.expect_read_file()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(move |_| Ok(content_string.clone()));

        // Set up path_exists to return true for this path
        self.expect_path_exists()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|_| true);

        // Set up expand_path to return the path as-is
        self.expect_expand_path()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|p| Ok(p.to_path_buf()));

        // Setup canonicalize to return the path as-is
        self.expect_canonicalize()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|p| Ok(p.to_path_buf()));

        // Add the parent directory to list_directory results if it doesn't exist
        if let Some(parent) = path.parent() {
            self.add_existing_path(parent);

            // Make the directory list include this file
            let parent_path = parent.to_path_buf();
            let file_path = path_buf.clone();

            self.expect_list_directory()
                .with(mockall::predicate::eq(parent_path))
                .returning(move |_| Ok(vec![file_path.clone()]));
        }
    }

    fn add_existing_path(&mut self, path: &Path) {
        let path_buf = path.to_path_buf();

        // Set up path_exists to return true for this path
        self.expect_path_exists()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|_| true);

        // Set up expand_path to return the path as-is
        self.expect_expand_path()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|p| Ok(p.to_path_buf()));

        // Setup canonicalize to return the path as-is
        self.expect_canonicalize()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|p| Ok(p.to_path_buf()));

        // Set up list_directory to return an empty list for this directory
        self.expect_list_directory()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(|_| Ok(vec![]));
    }
}
