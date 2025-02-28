// src/filesystem.rs

use std::{
    io,
    path::{Path, PathBuf},
};

use thiserror::Error;
use tokio::fs as tokio_fs;

#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

#[async_trait::async_trait]
pub trait FileSystem: Send + Sync {
    async fn read_file(&self, path: &Path) -> Result<String, FileSystemError>;
    async fn path_exists(&self, path: &Path) -> bool;
    async fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError>;
    async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError>;
}

// Implement FileSystem for references to types that implement FileSystem
#[async_trait::async_trait]
impl<T: FileSystem + ?Sized> FileSystem for &T {
    async fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
        (*self).read_file(path).await
    }

    async fn path_exists(&self, path: &Path) -> bool {
        (*self).path_exists(path).await
    }

    async fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
        (*self).expand_path(path).await
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
        (*self).list_directory(path).await
    }
}

pub struct RealFileSystem;

#[async_trait::async_trait]
impl FileSystem for RealFileSystem {
    async fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
        tokio_fs::read_to_string(path)
            .await
            .map_err(FileSystemError::IoError)
    }

    async fn path_exists(&self, path: &Path) -> bool {
        tokio_fs::metadata(path).await.is_ok()
    }

    async fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
        let binding = path.to_string_lossy();
        let expanded = shellexpand::tilde(&binding);
        let expanded_path = PathBuf::from(expanded.as_ref());

        // We need to check if the path exists first
        if tokio_fs::metadata(&expanded_path).await.is_err() {
            return Err(FileSystemError::PathNotFound(
                path.to_string_lossy().into_owned(),
            ));
        }

        // Use blocking for canonicalize since tokio doesn't have this
        tokio::task::spawn_blocking(move || {
            expanded_path.canonicalize().map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    FileSystemError::PathNotFound(expanded_path.to_string_lossy().into_owned())
                } else {
                    FileSystemError::IoError(e)
                }
            })
        })
        .await
        .unwrap()
    }

    async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
        let mut entries = tokio_fs::read_dir(path).await.map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                FileSystemError::PathNotFound(path.to_string_lossy().into_owned())
            } else {
                FileSystemError::IoError(e)
            }
        })?;

        let mut paths = Vec::new();
        while let Some(entry) = entries
            .next_entry()
            .await
            .map_err(FileSystemError::IoError)?
        {
            paths.push(entry.path());
        }

        Ok(paths)
    }
}

pub mod mock {
    use super::*;
    use std::collections::HashMap;
    use std::sync::{Arc, Mutex};

    #[derive(Default, Clone)]
    pub struct MockFileSystem {
        files: Arc<Mutex<HashMap<PathBuf, String>>>,
        existing_paths: Arc<Mutex<Vec<PathBuf>>>,
    }

    impl MockFileSystem {
        pub fn add_file(&self, path: &Path, content: &str) {
            self.files
                .lock()
                .unwrap()
                .insert(path.to_path_buf(), content.to_string());
            self.existing_paths.lock().unwrap().push(path.to_path_buf());

            // Also add the parent directory to existing paths if it doesn't exist
            if let Some(parent) = path.parent() {
                let mut paths = self.existing_paths.lock().unwrap();
                if !paths.contains(&parent.to_path_buf()) {
                    paths.push(parent.to_path_buf());
                }
            }
        }

        pub fn add_existing_path(&self, path: &Path) {
            self.existing_paths.lock().unwrap().push(path.to_path_buf());
        }
    }

    #[async_trait::async_trait]
    impl FileSystem for MockFileSystem {
        async fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
            self.files
                .lock()
                .unwrap()
                .get(path)
                .cloned()
                .ok_or_else(|| FileSystemError::PathNotFound(path.to_string_lossy().into_owned()))
        }

        async fn path_exists(&self, path: &Path) -> bool {
            self.existing_paths
                .lock()
                .unwrap()
                .contains(&path.to_path_buf())
        }

        async fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
            // For simplicity, just return the path as-is in the mock
            Ok(path.to_path_buf())
        }

        async fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
            if !self.path_exists(path).await {
                return Err(FileSystemError::PathNotFound(
                    path.to_string_lossy().into_owned(),
                ));
            }

            // For the mock, return any files we've added that are in this directory
            let mut paths = Vec::new();

            // Check all files to see if they're in this directory
            for file_path in self.files.lock().unwrap().keys() {
                if let Some(parent) = file_path.parent() {
                    if parent == path {
                        paths.push(file_path.clone());
                    }
                }
            }

            Ok(paths)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::filesystem::mock::MockFileSystem;

    #[tokio::test]
    async fn test_real_file_system() {
        let fs = RealFileSystem;

        // Test path_exists
        assert!(fs.path_exists(Path::new("/")).await);
        assert!(
            !fs.path_exists(Path::new("/this_path_should_not_exist"))
                .await
        );

        // Test expand_path - we'll skip this in automated tests as it depends on the host system
        // and can be unreliable in CI environments
    }

    #[tokio::test]
    async fn test_mock_file_system() {
        let fs = MockFileSystem::default();

        // Add a mock file
        fs.add_file(Path::new("/test.txt"), "Hello, World!");

        // Test read_file
        assert_eq!(
            fs.read_file(Path::new("/test.txt")).await.unwrap(),
            "Hello, World!"
        );

        // Test path_exists
        assert!(fs.path_exists(Path::new("/test.txt")).await);
        assert!(!fs.path_exists(Path::new("/nonexistent.txt")).await);

        // Test expand_path
        assert_eq!(
            fs.expand_path(Path::new("/test/path")).await.unwrap(),
            PathBuf::from("/test/path")
        );
    }

    #[tokio::test]
    async fn test_mock_file_system_errors() {
        let fs = MockFileSystem::default();

        // Test read_file error
        assert!(matches!(
            fs.read_file(Path::new("/nonexistent.txt")).await,
            Err(FileSystemError::PathNotFound(_))
        ));
    }

    #[tokio::test]
    async fn test_reference_implementation() {
        let fs = MockFileSystem::default();
        let fs_ref = &fs;

        // Add a mock file to the original filesystem
        fs.add_file(Path::new("/test.txt"), "Hello, World!");

        // Test that the reference implementation works
        assert!(fs_ref.path_exists(Path::new("/test.txt")).await);
        assert_eq!(
            fs_ref.read_file(Path::new("/test.txt")).await.unwrap(),
            "Hello, World!"
        );
    }

    #[tokio::test]
    async fn test_list_directory() {
        let fs = MockFileSystem::default();

        // Add some files in a directory
        let dir = Path::new("/test/dir");
        fs.add_existing_path(dir);

        let file1 = dir.join("file1.txt");
        let file2 = dir.join("file2.yaml");
        let file3 = dir.join("file3.yml");

        fs.add_file(&file1, "contents1");
        fs.add_file(&file2, "contents2");
        fs.add_file(&file3, "contents3");

        // List the directory
        let files = fs.list_directory(dir).await.unwrap();

        // Check that all files are listed
        assert_eq!(files.len(), 3);
        assert!(files.contains(&file1));
        assert!(files.contains(&file2));
        assert!(files.contains(&file3));

        // Test a non-existent directory
        let result = fs.list_directory(Path::new("/nonexistent")).await;
        assert!(result.is_err());
        assert!(matches!(result, Err(FileSystemError::PathNotFound(_))));
    }
}
