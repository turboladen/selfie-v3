// src/ports/filesystem.rs
// File system port (interface)

use std::{
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;

/// Port for file system operations
#[cfg_attr(test, mockall::automock)]
#[async_trait::async_trait]
pub trait FileSystem: Send + Sync {
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

    fn config_dir(&self) -> Result<PathBuf, FileSystemError>;
}

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

#[cfg(test)]
impl MockFileSystem {
    pub(crate) fn mock_read_file<P, S>(&mut self, path: P, content: S)
    where
        PathBuf: From<P>,
        S: ToString,
    {
        let path_buf = PathBuf::from(path);
        let content_string = content.to_string();
        self.expect_read_file()
            .with(mockall::predicate::eq(path_buf.clone()))
            .returning(move |_| Ok(content_string.clone()));
    }

    pub(crate) fn mock_list_directory<P>(&mut self, path: P, entries: &[P])
    where
        PathBuf: From<P>,
        P: Clone + Sync,
    {
        let dir = PathBuf::from(path);
        let paths: Vec<_> = entries.iter().cloned().map(|e| PathBuf::from(e)).collect();

        self.expect_list_directory()
            .with(mockall::predicate::eq(dir.clone()))
            .returning(move |_| Ok(paths.clone()));
    }

    pub(crate) fn mock_path_exists<P>(&mut self, path: P, exists: bool)
    where
        PathBuf: From<P>,
    {
        self.expect_path_exists()
            .with(mockall::predicate::eq(PathBuf::from(path)))
            .returning(move |_| exists);
    }

    pub(crate) fn mock_config_dir<P>(&mut self, path: P)
    where
        PathBuf: From<P>,
    {
        let p = PathBuf::from(path);
        self.expect_config_dir().return_once(|| Ok(p));
    }

    pub(crate) fn mock_expand_path<P>(&mut self, input: P, output: P)
    where
        PathBuf: From<P>,
    {
        let input = PathBuf::from(input);
        let output = PathBuf::from(output);

        self.expect_expand_path()
            .with(mockall::predicate::eq(input))
            .return_once(|_| Ok(output));
    }
}
