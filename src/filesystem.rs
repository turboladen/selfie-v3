// src/filesystem.rs - Update with list_directory method

use std::fs;
use std::io;
use std::path::{Path, PathBuf};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum FileSystemError {
    #[error("IO error: {0}")]
    IoError(#[from] io::Error),

    #[error("Path not found: {0}")]
    PathNotFound(String),

    #[error("Invalid path: {0}")]
    InvalidPath(String),
}

pub trait FileSystem {
    fn read_file(&self, path: &Path) -> Result<String, FileSystemError>;
    fn path_exists(&self, path: &Path) -> bool;
    fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError>;

    // New method to list directory contents
    fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError>;
}

// Implement FileSystem for references to types that implement FileSystem
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
}

pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
        fs::read_to_string(path).map_err(FileSystemError::IoError)
    }

    fn path_exists(&self, path: &Path) -> bool {
        path.exists()
    }

    fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
        let binding = path.to_string_lossy();
        let expanded = shellexpand::tilde(&binding);
        PathBuf::from(expanded.as_ref())
            .canonicalize()
            .map_err(|e| {
                if e.kind() == io::ErrorKind::NotFound {
                    FileSystemError::PathNotFound(path.to_string_lossy().into_owned())
                } else {
                    FileSystemError::IoError(e)
                }
            })
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
        let entries = fs::read_dir(path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                FileSystemError::PathNotFound(path.to_string_lossy().into_owned())
            } else {
                FileSystemError::IoError(e)
            }
        })?;

        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(FileSystemError::IoError)?;
            paths.push(entry.path());
        }

        Ok(paths)
    }
}

pub mod mock {
    use super::*;
    use std::cell::RefCell;
    use std::collections::HashMap;

    #[derive(Default, Clone)]
    pub struct MockFileSystem {
        files: RefCell<HashMap<PathBuf, String>>,
        existing_paths: RefCell<Vec<PathBuf>>,
    }

    impl MockFileSystem {
        pub fn add_file(&self, path: &Path, content: &str) {
            self.files
                .borrow_mut()
                .insert(path.to_path_buf(), content.to_string());
            self.existing_paths.borrow_mut().push(path.to_path_buf());

            // Also add the parent directory to existing paths if it doesn't exist
            if let Some(parent) = path.parent() {
                if !self.existing_paths.borrow().contains(&parent.to_path_buf()) {
                    self.existing_paths.borrow_mut().push(parent.to_path_buf());
                }
            }
        }

        pub fn add_existing_path(&self, path: &Path) {
            self.existing_paths.borrow_mut().push(path.to_path_buf());
        }
    }

    impl FileSystem for MockFileSystem {
        fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
            self.files
                .borrow()
                .get(path)
                .cloned()
                .ok_or_else(|| FileSystemError::PathNotFound(path.to_string_lossy().into_owned()))
        }

        fn path_exists(&self, path: &Path) -> bool {
            self.existing_paths.borrow().contains(&path.to_path_buf())
        }

        fn expand_path(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
            // For simplicity, just return the path as-is in the mock
            Ok(path.to_path_buf())
        }

        fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
            if !self.path_exists(path) {
                return Err(FileSystemError::PathNotFound(
                    path.to_string_lossy().into_owned(),
                ));
            }

            // For the mock, return any files we've added that are in this directory
            let mut paths = Vec::new();

            // Check all files to see if they're in this directory
            for file_path in self.files.borrow().keys() {
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

    #[test]
    fn test_real_file_system() {
        let fs = RealFileSystem;

        // Test path_exists
        assert!(fs.path_exists(Path::new("/")));
        assert!(!fs.path_exists(Path::new("/this_path_should_not_exist")));

        // Test expand_path
        let home_dir = dirs::home_dir().unwrap();
        assert_eq!(fs.expand_path(Path::new("~/")).unwrap(), home_dir);
    }

    #[test]
    fn test_mock_file_system() {
        let fs = MockFileSystem::default();

        // Add a mock file
        fs.add_file(Path::new("/test.txt"), "Hello, World!");

        // Test read_file
        assert_eq!(
            fs.read_file(Path::new("/test.txt")).unwrap(),
            "Hello, World!"
        );

        // Test path_exists
        assert!(fs.path_exists(Path::new("/test.txt")));
        assert!(!fs.path_exists(Path::new("/nonexistent.txt")));

        // Test expand_path
        assert_eq!(
            fs.expand_path(Path::new("/test/path")).unwrap(),
            PathBuf::from("/test/path")
        );
    }

    #[test]
    fn test_mock_file_system_errors() {
        let fs = MockFileSystem::default();

        // Test read_file error
        assert!(matches!(
            fs.read_file(Path::new("/nonexistent.txt")),
            Err(FileSystemError::PathNotFound(_))
        ));
    }

    #[test]
    fn test_reference_implementation() {
        let fs = MockFileSystem::default();
        let fs_ref = &fs;

        // Add a mock file to the original filesystem
        fs.add_file(Path::new("/test.txt"), "Hello, World!");

        // Test that the reference implementation works
        assert!(fs_ref.path_exists(Path::new("/test.txt")));
        assert_eq!(
            fs_ref.read_file(Path::new("/test.txt")).unwrap(),
            "Hello, World!"
        );
    }

    #[test]
    fn test_list_directory() {
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
        let files = fs.list_directory(dir).unwrap();

        // Check that all files are listed
        assert_eq!(files.len(), 3);
        assert!(files.contains(&file1));
        assert!(files.contains(&file2));
        assert!(files.contains(&file3));

        // Test a non-existent directory
        let result = fs.list_directory(Path::new("/nonexistent"));
        assert!(result.is_err());
        assert!(matches!(result, Err(FileSystemError::PathNotFound(_))));
    }
}
