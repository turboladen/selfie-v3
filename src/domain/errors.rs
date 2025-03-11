// src/domain/errors.rs
// Enhanced error types with context and formatting capabilities

use std::fmt;
use std::path::{Path, PathBuf};
use thiserror::Error;

/// Error context provides additional information about where and why an error occurred
#[derive(Debug, Clone, Default)]
pub struct ErrorContext {
    /// The file or path associated with the error
    pub path: Option<PathBuf>,

    /// The command that caused the error
    pub command: Option<String>,

    /// The environment where the error occurred
    pub environment: Option<String>,

    /// The package associated with the error
    pub package: Option<String>,

    /// Line number in a file where the error occurred
    pub line: Option<usize>,

    /// Additional context message
    pub message: Option<String>,
}

impl ErrorContext {
    /// Add a path to the context
    pub fn with_path<P: AsRef<Path>>(mut self, path: P) -> Self {
        self.path = Some(PathBuf::from(path.as_ref()));
        self
    }

    /// Add a command to the context
    pub fn with_command(mut self, command: &str) -> Self {
        self.command = Some(command.to_string());
        self
    }

    /// Add an environment to the context
    pub fn with_environment(mut self, environment: &str) -> Self {
        self.environment = Some(environment.to_string());
        self
    }

    /// Add a package to the context
    pub fn with_package(mut self, package: &str) -> Self {
        self.package = Some(package.to_string());
        self
    }

    /// Add a line number to the context
    pub fn with_line(mut self, line: usize) -> Self {
        self.line = Some(line);
        self
    }

    /// Add a message to the context
    pub fn with_message(mut self, message: &str) -> Self {
        self.message = Some(message.to_string());
        self
    }
}

impl fmt::Display for ErrorContext {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut has_content = false;

        if let Some(path) = &self.path {
            write!(f, "Path: {}", path.display())?;
            has_content = true;
        }

        if let Some(command) = &self.command {
            if has_content {
                write!(f, ", ")?;
            }
            write!(f, "Command: {}", command)?;
            has_content = true;
        }

        if let Some(environment) = &self.environment {
            if has_content {
                write!(f, ", ")?;
            }
            write!(f, "Environment: {}", environment)?;
            has_content = true;
        }

        if let Some(package) = &self.package {
            if has_content {
                write!(f, ", ")?;
            }
            write!(f, "Package: {}", package)?;
            has_content = true;
        }

        if let Some(line) = &self.line {
            if has_content {
                write!(f, ", ")?;
            }
            write!(f, "Line: {}", line)?;
            has_content = true;
        }

        if let Some(message) = &self.message {
            if has_content {
                write!(f, ", ")?;
            }
            write!(f, "{}", message)?;
        }

        Ok(())
    }
}

/// Enhanced file system error with context
#[derive(Error, Debug)]
pub enum EnhancedFileSystemError {
    #[error("Path not found: {path}")]
    PathNotFound {
        path: PathBuf,
        context: ErrorContext,
    },

    #[error("Permission denied: {path}")]
    PermissionDenied {
        path: PathBuf,
        context: ErrorContext,
    },

    #[error("IO error: {message}")]
    IoError {
        message: String,
        source: std::io::Error,
        context: ErrorContext,
    },

    #[error("Invalid path: {path}")]
    InvalidPath {
        path: PathBuf,
        context: ErrorContext,
    },
}

impl EnhancedFileSystemError {
    /// Create a path not found error
    pub fn path_not_found<P: AsRef<Path>>(path: P) -> Self {
        Self::PathNotFound {
            path: PathBuf::from(path.as_ref()),
            context: ErrorContext::default(),
        }
    }

    /// Create a permission denied error
    pub fn permission_denied<P: AsRef<Path>>(path: P) -> Self {
        Self::PermissionDenied {
            path: PathBuf::from(path.as_ref()),
            context: ErrorContext::default(),
        }
    }

    /// Create an IO error
    pub fn io_error(error: std::io::Error) -> Self {
        Self::IoError {
            message: error.to_string(),
            source: error,
            context: ErrorContext::default(),
        }
    }

    /// Create an invalid path error
    pub fn invalid_path<P: AsRef<Path>>(path: P) -> Self {
        Self::InvalidPath {
            path: PathBuf::from(path.as_ref()),
            context: ErrorContext::default(),
        }
    }

    /// Add context to the error
    pub fn with_context(self, context: ErrorContext) -> Self {
        match self {
            Self::PathNotFound { path, .. } => Self::PathNotFound { path, context },
            Self::PermissionDenied { path, .. } => Self::PermissionDenied { path, context },
            Self::IoError {
                message, source, ..
            } => Self::IoError {
                message,
                source,
                context,
            },
            Self::InvalidPath { path, .. } => Self::InvalidPath { path, context },
        }
    }

    /// Get the context of the error
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::PathNotFound { context, .. } => context,
            Self::PermissionDenied { context, .. } => context,
            Self::IoError { context, .. } => context,
            Self::InvalidPath { context, .. } => context,
        }
    }
}

/// Enhanced package error with context
#[derive(Error, Debug)]
pub enum EnhancedPackageError {
    #[error("Package not found: {name}")]
    PackageNotFound { name: String, context: ErrorContext },

    #[error("Multiple packages found with name: {name}")]
    MultiplePackagesFound { name: String, context: ErrorContext },

    #[error("Package validation error: {message}")]
    ValidationError {
        message: String,
        context: ErrorContext,
    },

    #[error("Package parse error: {message}")]
    ParseError {
        message: String,
        context: ErrorContext,
    },

    #[error("Environment not supported: {environment} for package {package}")]
    EnvironmentNotSupported {
        environment: String,
        package: String,
        context: ErrorContext,
    },
}

impl EnhancedPackageError {
    /// Create a package not found error
    pub fn package_not_found(name: &str) -> Self {
        Self::PackageNotFound {
            name: name.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create a multiple packages found error
    pub fn multiple_packages_found(name: &str) -> Self {
        Self::MultiplePackagesFound {
            name: name.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create a validation error
    pub fn validation_error(message: &str) -> Self {
        Self::ValidationError {
            message: message.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create a parse error
    pub fn parse_error(message: &str) -> Self {
        Self::ParseError {
            message: message.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create an environment not supported error
    pub fn environment_not_supported(environment: &str, package: &str) -> Self {
        Self::EnvironmentNotSupported {
            environment: environment.to_string(),
            package: package.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Add context to the error
    pub fn with_context(self, context: ErrorContext) -> Self {
        match self {
            Self::PackageNotFound { name, .. } => Self::PackageNotFound { name, context },
            Self::MultiplePackagesFound { name, .. } => {
                Self::MultiplePackagesFound { name, context }
            }
            Self::ValidationError { message, .. } => Self::ValidationError { message, context },
            Self::ParseError { message, .. } => Self::ParseError { message, context },
            Self::EnvironmentNotSupported {
                environment,
                package,
                ..
            } => Self::EnvironmentNotSupported {
                environment,
                package,
                context,
            },
        }
    }

    /// Get the context of the error
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::PackageNotFound { context, .. } => context,
            Self::MultiplePackagesFound { context, .. } => context,
            Self::ValidationError { context, .. } => context,
            Self::ParseError { context, .. } => context,
            Self::EnvironmentNotSupported { context, .. } => context,
        }
    }
}

/// Enhanced command execution error with context
#[derive(Error, Debug)]
pub enum EnhancedCommandError {
    #[error("Command execution failed: {command}")]
    ExecutionFailed {
        command: String,
        exit_code: i32,
        stdout: String,
        stderr: String,
        context: ErrorContext,
    },

    #[error("Command timed out after {timeout} seconds: {command}")]
    Timeout {
        command: String,
        timeout: u64,
        context: ErrorContext,
    },

    #[error("Command interrupted: {command}")]
    Interrupted {
        command: String,
        context: ErrorContext,
    },

    #[error("Permission denied: {command}")]
    PermissionDenied {
        command: String,
        context: ErrorContext,
    },

    #[error("Command not found: {command}")]
    CommandNotFound {
        command: String,
        context: ErrorContext,
    },
}

impl EnhancedCommandError {
    /// Create an execution failed error
    pub fn execution_failed(command: &str, exit_code: i32, stdout: &str, stderr: &str) -> Self {
        Self::ExecutionFailed {
            command: command.to_string(),
            exit_code,
            stdout: stdout.to_string(),
            stderr: stderr.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create a timeout error
    pub fn timeout(command: &str, timeout: u64) -> Self {
        Self::Timeout {
            command: command.to_string(),
            timeout,
            context: ErrorContext::default(),
        }
    }

    /// Create an interrupted error
    pub fn interrupted(command: &str) -> Self {
        Self::Interrupted {
            command: command.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create a permission denied error
    pub fn permission_denied(command: &str) -> Self {
        Self::PermissionDenied {
            command: command.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create a command not found error
    pub fn command_not_found(command: &str) -> Self {
        Self::CommandNotFound {
            command: command.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Add context to the error
    pub fn with_context(self, context: ErrorContext) -> Self {
        match self {
            Self::ExecutionFailed {
                command,
                exit_code,
                stdout,
                stderr,
                ..
            } => Self::ExecutionFailed {
                command,
                exit_code,
                stdout,
                stderr,
                context,
            },
            Self::Timeout {
                command, timeout, ..
            } => Self::Timeout {
                command,
                timeout,
                context,
            },
            Self::Interrupted { command, .. } => Self::Interrupted { command, context },
            Self::PermissionDenied { command, .. } => Self::PermissionDenied { command, context },
            Self::CommandNotFound { command, .. } => Self::CommandNotFound { command, context },
        }
    }

    /// Get the context of the error
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::ExecutionFailed { context, .. } => context,
            Self::Timeout { context, .. } => context,
            Self::Interrupted { context, .. } => context,
            Self::PermissionDenied { context, .. } => context,
            Self::CommandNotFound { context, .. } => context,
        }
    }
}

/// Enhanced dependency error with context
#[derive(Error, Debug)]
pub enum EnhancedDependencyError {
    #[error("Circular dependency detected: {cycle}")]
    CircularDependency {
        cycle: String,
        path: Vec<String>,
        context: ErrorContext,
    },

    #[error("Missing dependency: {name}")]
    MissingDependency { name: String, context: ErrorContext },

    #[error("Dependency environment mismatch: {dependency} for {package}")]
    EnvironmentMismatch {
        dependency: String,
        package: String,
        environment: String,
        context: ErrorContext,
    },
}

impl EnhancedDependencyError {
    /// Create a circular dependency error
    pub fn circular_dependency(path: Vec<String>) -> Self {
        let cycle = path.join(" → ");
        Self::CircularDependency {
            cycle,
            path,
            context: ErrorContext::default(),
        }
    }

    /// Create a missing dependency error
    pub fn missing_dependency(name: &str) -> Self {
        Self::MissingDependency {
            name: name.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Create an environment mismatch error
    pub fn environment_mismatch(dependency: &str, package: &str, environment: &str) -> Self {
        Self::EnvironmentMismatch {
            dependency: dependency.to_string(),
            package: package.to_string(),
            environment: environment.to_string(),
            context: ErrorContext::default(),
        }
    }

    /// Add context to the error
    pub fn with_context(self, context: ErrorContext) -> Self {
        match self {
            Self::CircularDependency { cycle, path, .. } => Self::CircularDependency {
                cycle,
                path,
                context,
            },
            Self::MissingDependency { name, .. } => Self::MissingDependency { name, context },
            Self::EnvironmentMismatch {
                dependency,
                package,
                environment,
                ..
            } => Self::EnvironmentMismatch {
                dependency,
                package,
                environment,
                context,
            },
        }
    }

    /// Get the context of the error
    pub fn context(&self) -> &ErrorContext {
        match self {
            Self::CircularDependency { context, .. } => context,
            Self::MissingDependency { context, .. } => context,
            Self::EnvironmentMismatch { context, .. } => context,
        }
    }

    /// Get the dependency cycle path
    pub fn cycle_path(&self) -> Option<&Vec<String>> {
        match self {
            Self::CircularDependency { path, .. } => Some(path),
            _ => None,
        }
    }
}

/// Extension trait for adding context to errors
pub trait WithContext<T> {
    /// Add context to an error
    fn with_context<F>(self, f: F) -> T
    where
        F: FnOnce() -> ErrorContext;
}

impl<T, E> WithContext<Result<T, EnhancedFileSystemError>> for Result<T, E>
where
    E: Into<EnhancedFileSystemError>,
{
    fn with_context<F>(self, f: F) -> Result<T, EnhancedFileSystemError>
    where
        F: FnOnce() -> ErrorContext,
    {
        self.map_err(|e| {
            let err = e.into();
            err.with_context(f())
        })
    }
}

impl<T, E> WithContext<Result<T, EnhancedPackageError>> for Result<T, E>
where
    E: Into<EnhancedPackageError>,
{
    fn with_context<F>(self, f: F) -> Result<T, EnhancedPackageError>
    where
        F: FnOnce() -> ErrorContext,
    {
        self.map_err(|e| {
            let err = e.into();
            err.with_context(f())
        })
    }
}

impl<T, E> WithContext<Result<T, EnhancedCommandError>> for Result<T, E>
where
    E: Into<EnhancedCommandError>,
{
    fn with_context<F>(self, f: F) -> Result<T, EnhancedCommandError>
    where
        F: FnOnce() -> ErrorContext,
    {
        self.map_err(|e| {
            let err = e.into();
            err.with_context(f())
        })
    }
}

impl<T, E> WithContext<Result<T, EnhancedDependencyError>> for Result<T, E>
where
    E: Into<EnhancedDependencyError>,
{
    fn with_context<F>(self, f: F) -> Result<T, EnhancedDependencyError>
    where
        F: FnOnce() -> ErrorContext,
    {
        self.map_err(|e| {
            let err = e.into();
            err.with_context(f())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_error_context() {
        let context = ErrorContext::default()
            .with_path("/test/path")
            .with_command("test command")
            .with_environment("test-env")
            .with_package("test-package")
            .with_line(42)
            .with_message("Additional info");

        let context_str = context.to_string();

        assert!(context_str.contains("/test/path"));
        assert!(context_str.contains("test command"));
        assert!(context_str.contains("test-env"));
        assert!(context_str.contains("test-package"));
        assert!(context_str.contains("42"));
        assert!(context_str.contains("Additional info"));
    }

    #[test]
    fn test_enhanced_file_system_error() {
        let error = EnhancedFileSystemError::path_not_found("/test/path");
        assert!(error.to_string().contains("Path not found"));

        let context = ErrorContext::default().with_package("test-package");
        let error = error.with_context(context);

        match &error {
            EnhancedFileSystemError::PathNotFound { context, .. } => {
                assert_eq!(context.package, Some("test-package".to_string()));
            }
            _ => panic!("Expected PathNotFound variant"),
        }
    }

    #[test]
    fn test_enhanced_package_error() {
        let error = EnhancedPackageError::package_not_found("test-package");
        assert!(error.to_string().contains("Package not found"));

        let context = ErrorContext::default().with_environment("test-env");
        let error = error.with_context(context);

        match &error {
            EnhancedPackageError::PackageNotFound { context, .. } => {
                assert_eq!(context.environment, Some("test-env".to_string()));
            }
            _ => panic!("Expected PackageNotFound variant"),
        }
    }

    #[test]
    fn test_enhanced_command_error() {
        let error = EnhancedCommandError::execution_failed("test command", 1, "stdout", "stderr");
        assert!(error.to_string().contains("Command execution failed"));

        let context = ErrorContext::default().with_environment("test-env");
        let error = error.with_context(context);

        match &error {
            EnhancedCommandError::ExecutionFailed { context, .. } => {
                assert_eq!(context.environment, Some("test-env".to_string()));
            }
            _ => panic!("Expected ExecutionFailed variant"),
        }
    }

    #[test]
    fn test_enhanced_dependency_error() {
        let path = vec![
            "package-a".to_string(),
            "package-b".to_string(),
            "package-a".to_string(),
        ];
        let error = EnhancedDependencyError::circular_dependency(path.clone());
        assert!(error.to_string().contains("Circular dependency detected"));

        let context = ErrorContext::default().with_environment("test-env");
        let error = error.with_context(context);

        match &error {
            EnhancedDependencyError::CircularDependency {
                context,
                path: error_path,
                ..
            } => {
                assert_eq!(context.environment, Some("test-env".to_string()));
                assert_eq!(error_path, &path);
            }
            _ => panic!("Expected CircularDependency variant"),
        }
    }

    #[test]
    fn test_with_context_trait() {
        // Can't easily test with actual error types due to trait bounds,
        // but we can test the behavior with a mock

        let mock_result: Result<(), EnhancedFileSystemError> =
            Err(EnhancedFileSystemError::path_not_found("/test/path"));

        let result =
            mock_result.with_context(|| ErrorContext::default().with_package("test-package"));

        match result {
            Err(EnhancedFileSystemError::PathNotFound { context, .. }) => {
                assert_eq!(context.package, Some("test-package".to_string()));
            }
            _ => panic!("Expected PathNotFound variant"),
        }
    }
}
