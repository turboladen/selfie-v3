// src/domain/validation.rs
use crate::domain::package::Package;
use std::{fmt, path::PathBuf};

/// Categories of package validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ValidationErrorCategory {
    /// Missing required fields
    RequiredField,
    /// Invalid field values
    InvalidValue,
    /// Environment-specific errors
    Environment,
    /// Shell command syntax errors
    CommandSyntax,
    /// URL format errors
    UrlFormat,
    /// File system errors
    FileSystem,
    /// Availability and compatibility errors
    Availability,
    /// Other errors
    Other,
}

impl fmt::Display for ValidationErrorCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ValidationErrorCategory::RequiredField => f.write_str("Required field"),
            ValidationErrorCategory::InvalidValue => f.write_str("Invalid value"),
            ValidationErrorCategory::Environment => f.write_str("Environment"),
            ValidationErrorCategory::CommandSyntax => f.write_str("Command syntax"),
            ValidationErrorCategory::UrlFormat => f.write_str("URL format"),
            ValidationErrorCategory::FileSystem => f.write_str("File system"),
            ValidationErrorCategory::Availability => f.write_str("Availability"),
            ValidationErrorCategory::Other => f.write_str("Other"),
        }
    }
}

/// A single validation issue (error or warning)
#[derive(Debug, Clone, PartialEq)]
pub struct ValidationIssue {
    /// The category of the issue
    pub category: ValidationErrorCategory,
    /// The field or context where the issue was found
    pub field: String,
    /// Detailed description of the issue
    pub message: String,
    /// Line number in the file (if available)
    pub line: Option<usize>,
    /// Is this a warning (false = error)
    pub is_warning: bool,
    /// Suggested fix for the issue
    pub suggestion: Option<String>,
}

impl ValidationIssue {
    /// Create a new validation error
    pub fn error(
        category: ValidationErrorCategory,
        field: &str,
        message: &str,
        line: Option<usize>,
        suggestion: Option<&str>,
    ) -> Self {
        Self {
            category,
            field: field.to_string(),
            message: message.to_string(),
            line,
            is_warning: false,
            suggestion: suggestion.map(|s| s.to_string()),
        }
    }

    /// Create a new validation warning
    pub fn warning(
        category: ValidationErrorCategory,
        field: &str,
        message: &str,
        line: Option<usize>,
        suggestion: Option<&str>,
    ) -> Self {
        Self {
            category,
            field: field.to_string(),
            message: message.to_string(),
            line,
            is_warning: true,
            suggestion: suggestion.map(|s| s.to_string()),
        }
    }
}

/// Results of a package validation
#[derive(Debug, Clone, Default)]
pub struct ValidationResult {
    /// The package that was validated
    pub package_name: String,
    /// The package file path
    pub package_path: Option<PathBuf>,
    /// List of validation issues found
    pub issues: Vec<ValidationIssue>,
    /// The validated package (if valid)
    pub package: Option<Package>,
}

impl ValidationResult {
    /// Create a new ValidationResult
    pub fn new(package_name: &str) -> Self {
        Self {
            package_name: package_name.to_string(),
            package_path: None,
            issues: Vec::new(),
            package: None,
        }
    }

    /// Add an issue to the validation result
    pub fn add_issue(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    /// Add multiple issues to the validation result
    pub fn add_issues(&mut self, issues: Vec<ValidationIssue>) {
        self.issues.extend(issues);
    }

    /// Set the package file path
    pub fn with_path(mut self, path: PathBuf) -> Self {
        self.package_path = Some(path);
        self
    }

    /// Set the validated package
    pub fn with_package(mut self, package: Package) -> Self {
        self.package = Some(package);
        self
    }

    /// Returns true if the validation passed (no errors)
    pub fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    /// Returns true if the validation has errors (warnings are okay)
    pub fn has_errors(&self) -> bool {
        self.issues.iter().any(|issue| !issue.is_warning)
    }

    /// Get all errors (not warnings)
    pub fn errors(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| !issue.is_warning)
            .collect()
    }

    /// Get all warnings (not errors)
    pub fn warnings(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.is_warning)
            .collect()
    }

    /// Get issues by category
    pub fn issues_by_category(&self, category: &ValidationErrorCategory) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.category == *category)
            .collect()
    }
}

/// Errors that can occur during validation operations
#[derive(thiserror::Error, Debug)]
pub enum ValidationError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Multiple packages found with name: {0}")]
    MultiplePackagesFound(String),

    #[error("Failed to parse package file: {0}")]
    ParseError(String),

    #[error("File system error: {0}")]
    FileSystemError(String),

    #[error("Command execution error: {0}")]
    CommandError(String),
}
