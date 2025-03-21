// src/domain/validation.rs
use std::{collections::HashMap, fmt, os::unix::fs::PermissionsExt, path::PathBuf};

use console::style;
use jiff::{fmt::temporal::SpanPrinter, Unit, Zoned};

use crate::{adapters::progress::ProgressManager, domain::package::Package};

/// Categories of package validation errors
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ValidationErrorCategory {
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
pub(crate) struct ValidationIssue {
    /// The category of the issue
    pub(crate) category: ValidationErrorCategory,
    /// The field or context where the issue was found
    pub(crate) field: String,
    /// Detailed description of the issue
    pub(crate) message: String,
    /// Line number in the file (if available)
    pub(crate) line: Option<usize>,
    /// Is this a warning (false = error)
    pub(crate) is_warning: bool,
    /// Suggested fix for the issue
    pub(crate) suggestion: Option<String>,
}

impl ValidationIssue {
    /// Create a new validation error
    pub(crate) fn error(
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
    pub(crate) fn warning(
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
#[derive(Debug, Clone, Default, PartialEq)]
pub(crate) struct ValidationResult {
    /// The package that was validated
    pub(crate) package_name: String,
    /// The package file path
    pub(crate) package_path: Option<PathBuf>,
    /// List of validation issues found
    pub(crate) issues: Vec<ValidationIssue>,
    /// The validated package (if valid)
    pub(crate) package: Option<Package>,
}

impl ValidationResult {
    /// Create a new ValidationResult
    pub(crate) fn new(package_name: &str) -> Self {
        Self {
            package_name: package_name.to_string(),
            package_path: None,
            issues: Vec::new(),
            package: None,
        }
    }

    /// Add an issue to the validation result
    pub(crate) fn add_issue(&mut self, issue: ValidationIssue) {
        self.issues.push(issue);
    }

    /// Add multiple issues to the validation result
    pub(crate) fn add_issues(&mut self, issues: Vec<ValidationIssue>) {
        self.issues.extend(issues);
    }

    /// Set the package file path
    pub(crate) fn with_path(mut self, path: PathBuf) -> Self {
        self.package_path = Some(path);
        self
    }

    /// Set the validated package
    pub(crate) fn with_package(mut self, package: Package) -> Self {
        self.package = Some(package);
        self
    }

    /// Returns true if the validation passed (no errors)
    pub(crate) fn is_valid(&self) -> bool {
        !self.has_errors()
    }

    /// Returns true if the validation has errors (warnings are okay)
    pub(crate) fn has_errors(&self) -> bool {
        self.issues.iter().any(|issue| !issue.is_warning)
    }

    /// Get all errors (not warnings)
    pub(crate) fn errors(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| !issue.is_warning)
            .collect()
    }

    /// Get all warnings (not errors)
    pub(crate) fn warnings(&self) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.is_warning)
            .collect()
    }

    /// Get issues by category
    pub(crate) fn issues_by_category(
        &self,
        category: &ValidationErrorCategory,
    ) -> Vec<&ValidationIssue> {
        self.issues
            .iter()
            .filter(|issue| issue.category == *category)
            .collect()
    }

    pub(crate) fn format_validation_result(&self, progress_manager: ProgressManager) -> String {
        let mut output = String::new();

        if self.is_valid() {
            let package_name = if progress_manager.use_colors() {
                style(&self.package_name).magenta().bold().to_string()
            } else {
                self.package_name.clone()
            };

            output.push_str(&format!("Package '{package_name}' is valid\n"));

            // Add warnings if any
            let warnings = self.warnings();
            if !warnings.is_empty() {
                let warning_header = if progress_manager.use_colors() {
                    style("Warnings:").yellow().bold().to_string()
                } else {
                    "Warnings:".to_string()
                };

                output.push_str(&format!("\n{}\n", warning_header));

                for warning in warnings {
                    let warn_prefix = if progress_manager.use_colors() {
                        style("  ! ").yellow().to_string()
                    } else {
                        "  ! ".to_string()
                    };

                    output.push_str(&format!(
                        "{}{}: {}\n",
                        warn_prefix, warning.field, warning.message
                    ));

                    if let Some(suggestion) = &warning.suggestion {
                        let suggestion_text = if progress_manager.use_colors() {
                            style(format!("    Suggestion: {}", suggestion))
                                .dim()
                                .to_string()
                        } else {
                            format!("    Suggestion: {}", suggestion)
                        };
                        output.push_str(&format!("{}\n", suggestion_text));
                    }
                }
            }
        } else {
            // Format logic for invalid package (stub this from the original implementation)
            // Similar to the valid case but shows errors by category
            self.format_invalid_result(
                &mut output,
                progress_manager.use_colors(),
                progress_manager.verbose(),
            );
        }

        output
    }

    // Helper method for formatting invalid validation results
    fn format_invalid_result(&self, output: &mut String, use_colors: bool, verbose: bool) {
        let status = if use_colors {
            style("✗").red().bold().to_string()
        } else {
            "✗".to_string()
        };

        let package_name = if use_colors {
            style(&self.package_name).magenta().bold().to_string()
        } else {
            self.package_name.clone()
        };

        output.push_str(&format!(
            "{} Validation failed for package: {}\n",
            status, package_name
        ));

        // Group errors by category
        let mut errors_by_category = HashMap::new();
        for error in self.errors() {
            errors_by_category
                .entry(error.category)
                .or_insert_with(Vec::new)
                .push(error);
        }

        // Add errors by category (stub - would include actual error formatting logic)
        // Display required field errors first
        if let Some(errors) = errors_by_category.get(&ValidationErrorCategory::RequiredField) {
            self.format_category_errors(output, "Required field errors", errors, use_colors);
        }

        // Then command syntax errors
        if let Some(errors) = errors_by_category.get(&ValidationErrorCategory::CommandSyntax) {
            self.format_category_errors(output, "Command syntax errors", errors, use_colors);
        }

        // Then URL format errors
        if let Some(errors) = errors_by_category.get(&ValidationErrorCategory::UrlFormat) {
            self.format_category_errors(output, "URL format errors", errors, use_colors);
        }

        // Then other categories
        for (category, errors) in &errors_by_category {
            if *category != ValidationErrorCategory::RequiredField
                && *category != ValidationErrorCategory::CommandSyntax
                && *category != ValidationErrorCategory::UrlFormat
            {
                self.format_category_errors(
                    output,
                    &format!("{:?} errors", category),
                    errors,
                    use_colors,
                );
            }
        }

        // Show file path
        if let Some(path) = &self.package_path {
            let path_text = if use_colors {
                style(format!(
                    "\nYou can find the package file at: {}",
                    path.display()
                ))
                .dim()
                .to_string()
            } else {
                format!("\nYou can find the package file at: {}", path.display())
            };

            output.push_str(&format!("{}\n", path_text));
        }

        // Add verbose information if requested
        if verbose {
            self.add_verbose_information(output, use_colors);
        }
    }

    // Helper to format category errors
    fn format_category_errors(
        &self,
        output: &mut String,
        header: &str,
        errors: &[&ValidationIssue],
        use_colors: bool,
    ) {
        let header_text = if use_colors {
            style(format!("\n{}:", header)).red().bold().to_string()
        } else {
            format!("\n{}:", header)
        };

        output.push_str(&header_text);
        output.push('\n');

        for error in errors {
            let field = if use_colors {
                style(&error.field).cyan().to_string()
            } else {
                error.field.clone()
            };

            output.push_str(&format!("  • {}: {}\n", field, error.message));

            if let Some(suggestion) = &error.suggestion {
                let suggestion_text = if use_colors {
                    style(format!("    Suggestion: {}", suggestion))
                        .dim()
                        .to_string()
                } else {
                    format!("    Suggestion: {}", suggestion)
                };
                output.push_str(&format!("{}\n", suggestion_text));
            }
        }
    }

    // Add verbose information to the output
    fn add_verbose_information(&self, output: &mut String, use_colors: bool) {
        output.push_str("\n--- Verbose Information ---\n");

        // Add file details
        if let Some(path) = &self.package_path {
            output.push_str("\nPackage file details:\n");
            output.push_str(&format!("  Path: {}\n", path.display()));

            if let Ok(metadata) = path.metadata() {
                output.push_str(&format!(
                    "  Permissions: {:o}\n",
                    metadata.permissions().mode()
                ));
                let now = Zoned::now();
                let printer = SpanPrinter::new();

                // Use use_colors for styling if needed
                let created_prefix = if use_colors {
                    style("  Created:").dim().to_string()
                } else {
                    "  Created:".to_string()
                };

                {
                    let created = Zoned::try_from(metadata.created().unwrap()).unwrap();

                    let ago = now
                        .duration_since(&created)
                        .round(Unit::Second)
                        .unwrap_or_default();

                    output.push_str(&format!(
                        "{} {:?} ({} ago)\n",
                        created_prefix,
                        &created.round(Unit::Second),
                        printer.duration_to_string(&ago)
                    ));
                }

                let modified_prefix = if use_colors {
                    style("  Modified:").dim().to_string()
                } else {
                    "  Modified:".to_string()
                };

                {
                    let modified = Zoned::try_from(metadata.modified().unwrap()).unwrap();

                    let ago = now
                        .duration_since(&modified)
                        .round(Unit::Second)
                        .unwrap_or_default();

                    output.push_str(&format!(
                        "{} {:?} ({} ago)\n",
                        modified_prefix,
                        &modified.round(Unit::Second),
                        printer.duration_to_string(&ago)
                    ));
                }
            }
        }

        // Add package details (stub)
        if let Some(package) = &self.package {
            output.push_str("\nPackage structure details:\n");
            output.push_str(&format!("  Name: {}\n", package.name));
            output.push_str(&format!("  Version: {}\n", package.version));

            output.push_str(&format!(
                "  Homepage: {}\n",
                package.homepage.as_deref().unwrap_or_default()
            ));

            output.push_str(&format!(
                "  Description: {}\n",
                package.description.as_deref().unwrap_or_default()
            ));

            output.push_str("  Environments:\n");

            for (name, env) in &package.environments {
                output.push_str(&format!("    {}:\n", name));

                output.push_str(&format!(
                    "      Check: {}\n",
                    &env.check.as_deref().unwrap_or_default()
                ));

                output.push_str(&format!("      Install: {}\n", &env.install));

                output.push_str("      Dependencies:\n");

                for dep in &env.dependencies {
                    output.push_str(&format!("        {}\n", &dep));
                }
            }
        }

        // Add validation statistics (stub)
        output.push_str("\nValidation statistics:\n");
        output.push_str(&format!("  Total issues: {}\n", self.issues.len()));
        output.push_str(&format!("  Errors: {}\n", self.errors().len()));
        output.push_str(&format!("  Warnings: {}\n", self.warnings().len()));
        output.push_str(&format!("  Is valid: {}\n", self.is_valid()));
    }
}

/// Errors that can occur during validation operations
#[derive(thiserror::Error, Debug)]
pub(crate) enum ValidationError {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_format_validation_result() {
        // Create a sample validation result with some issues
        let mut result = ValidationResult::new("test-package");

        // Add an error
        result.add_issue(ValidationIssue::error(
            ValidationErrorCategory::RequiredField,
            "name",
            "Package name is required",
            None,
            Some("Add 'name: your-package-name' to the package file."),
        ));

        // Add a warning
        result.add_issue(ValidationIssue::warning(
            ValidationErrorCategory::CommandSyntax,
            "install",
            "Command uses deprecated syntax",
            None,
            Some("Update to the newer syntax."),
        ));

        let progress_manager = ProgressManager::default();

        // Format the result
        let formatted = result.format_validation_result(progress_manager);

        // Check the output contains expected content
        assert!(formatted.contains("Validation failed"));
        assert!(formatted.contains("Package name is required"));
        assert!(formatted.contains("Add 'name: your-package-name'"));
    }
}
