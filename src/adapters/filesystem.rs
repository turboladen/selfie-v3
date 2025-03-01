// src/adapters/filesystem/real.rs
// Real file system adapter implementation

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

use crate::ports::filesystem::{FileSystem, FileSystemError};

/// Real file system implementation
pub struct RealFileSystem;

impl FileSystem for RealFileSystem {
    fn read_file(&self, path: &Path) -> Result<String, FileSystemError> {
        fs::read_to_string(path).map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => {
                FileSystemError::PathNotFound(path.to_string_lossy().to_string())
            }
            io::ErrorKind::PermissionDenied => {
                FileSystemError::PermissionDenied(path.to_string_lossy().to_string())
            }
            _ => FileSystemError::IoError(e),
        })
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
                    FileSystemError::PathNotFound(path.to_string_lossy().to_string())
                } else {
                    FileSystemError::IoError(e)
                }
            })
    }

    fn list_directory(&self, path: &Path) -> Result<Vec<PathBuf>, FileSystemError> {
        let entries = fs::read_dir(path).map_err(|e| {
            if e.kind() == io::ErrorKind::NotFound {
                FileSystemError::PathNotFound(path.to_string_lossy().to_string())
            } else if e.kind() == io::ErrorKind::PermissionDenied {
                FileSystemError::PermissionDenied(path.to_string_lossy().to_string())
            } else {
                FileSystemError::IoError(e)
            }
        })?;

        let mut paths = Vec::new();
        for entry in entries {
            let entry = entry.map_err(|e| FileSystemError::IoError(e))?;
            paths.push(entry.path());
        }

        Ok(paths)
    }

    fn canonicalize(&self, path: &Path) -> Result<PathBuf, FileSystemError> {
        path.canonicalize().map_err(|e| match e.kind() {
            io::ErrorKind::NotFound => {
                FileSystemError::PathNotFound(path.to_string_lossy().to_string())
            }
            io::ErrorKind::PermissionDenied => {
                FileSystemError::PermissionDenied(path.to_string_lossy().to_string())
            }
            _ => FileSystemError::IoError(e),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use tempfile::tempdir;

    #[test]
    fn test_path_exists() {
        let fs = RealFileSystem;

        // Create a temporary directory
        let dir = tempdir().unwrap();
        let file_path = dir.path().join("test.txt");

        // Path shouldn't exist yet
        assert!(!fs.path_exists(&file_path));

        // Create the file
        File::create(&file_path).unwrap();

        // Path should exist now
        assert!(fs.path_exists(&file_path));
    }

    #[test]
    fn test_list_directory() {
        let fs = RealFileSystem;

        // Create a temporary directory
        let dir = tempdir().unwrap();

        // Create some files
        let file1 = dir.path().join("file1.txt");
        let file2 = dir.path().join("file2.txt");

        File::create(&file1).unwrap();
        File::create(&file2).unwrap();

        // List directory
        let paths = fs.list_directory(dir.path()).unwrap();

        // Verify both files are listed
        assert_eq!(paths.len(), 2);
        assert!(paths.contains(&file1));
        assert!(paths.contains(&file2));
    }
}
