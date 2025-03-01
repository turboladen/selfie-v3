// src/package_validator.rs
// Provides package validation functionality with detailed error reporting

use std::{
    collections::HashMap,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};

use console::style;
use jiff::{fmt::friendly::SpanPrinter, Unit, Zoned};
use thiserror::Error;
use url::Url;

use crate::{
    config::Config,
    filesystem::{FileSystem, FileSystemError},
    domain::package::{EnvironmentConfig, Package, PackageParseError},
    package_repo::{PackageRepoError, PackageRepository},
};

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
    /// Other errors
    Other,
}

/// A single validation error or warning
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

#[derive(Error, Debug)]
pub enum PackageValidatorError {
    #[error("Package not found: {0}")]
    PackageNotFound(String),

    #[error("Multiple packages found with name: {0}")]
    MultiplePackagesFound(String),

    #[error("Invalid package: {0}")]
    InvalidPackage(String),

    #[error("Package repository error: {0}")]
    RepoError(#[from] PackageRepoError),

    #[error("File system error: {0}")]
    FileSystemError(#[from] FileSystemError),

    #[error("Package parse error: {0}")]
    ParseError(#[from] PackageParseError),
}

/// Validates package files with detailed error reporting
pub struct PackageValidator<'a, F: FileSystem> {
    fs: &'a F,
    config: &'a Config,
    package_repo: PackageRepository<'a, F>,
}

impl<'a, F: FileSystem> PackageValidator<'a, F> {
    /// Create a new package validator
    pub fn new(fs: &'a F, config: &'a Config) -> Self {
        let package_repo = PackageRepository::new(fs, config.expanded_package_directory());
        Self {
            fs,
            config,
            package_repo,
        }
    }

    /// Validate a package by name
    pub fn validate_package(
        &self,
        package_name: &str,
    ) -> Result<ValidationResult, PackageValidatorError> {
        // Find the package file
        let package_files = self
            .package_repo
            .find_package_files(package_name)
            .map_err(PackageValidatorError::RepoError)?;

        if package_files.is_empty() {
            return Err(PackageValidatorError::PackageNotFound(
                package_name.to_string(),
            ));
        }

        if package_files.len() > 1 {
            return Err(PackageValidatorError::MultiplePackagesFound(
                package_name.to_string(),
            ));
        }

        let package_path = &package_files[0];
        self.validate_package_file(package_path)
    }

    /// Validate a specific package file
    pub fn validate_package_file(
        &self,
        package_path: &Path,
    ) -> Result<ValidationResult, PackageValidatorError> {
        // Read and parse the package file
        let file_content = self
            .fs
            .read_file(package_path)
            .map_err(PackageValidatorError::FileSystemError)?;

        // Try to parse the package, but continue even if it fails
        let package = Package::from_yaml(&file_content);

        // Get the package name either from the parsed package or the file name
        let package_name = match &package {
            Ok(pkg) => pkg.name.clone(),
            Err(_) => package_path
                .file_stem()
                .and_then(|s| s.to_str())
                .unwrap_or("unknown")
                .to_string(),
        };

        let mut result = ValidationResult::new(&package_name).with_path(package_path.to_path_buf());

        // If parsing failed, add the parse error and return early
        match package {
            Ok(pkg) => {
                // Package parsed, so validate it deeply
                self.validate_package_fields(&pkg, &mut result);
                result = result.with_package(pkg);
            }
            Err(err) => {
                // Add the parse error
                result.add_issue(ValidationIssue::error(
                    ValidationErrorCategory::Other,
                    "package",
                    &format!("Failed to parse package file: {}", err),
                    None,
                    Some("Check the YAML format and fix the syntax errors."),
                ));
            }
        }

        Ok(result)
    }

    /// Validate package fields in detail
    fn validate_package_fields(&self, package: &Package, result: &mut ValidationResult) {
        // Validate required fields
        self.validate_required_fields(package, result);

        // Validate homepage URL if present
        if let Some(homepage) = &package.homepage {
            self.validate_url(homepage, "homepage", result);
        }

        // Validate environments
        self.validate_environments(package, result);
    }

    /// Validate required package fields
    fn validate_required_fields(&self, package: &Package, result: &mut ValidationResult) {
        // Check name
        if package.name.is_empty() {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                "name",
                "Package name is required",
                None,
                Some("Add 'name: your-package-name' to the package file."),
            ));
        } else if !Self::is_valid_package_name(&package.name) {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::InvalidValue,
                "name",
                "Package name contains invalid characters",
                None,
                Some("Use only alphanumeric characters, hyphens, and underscores."),
            ));
        }

        // Check version
        if package.version.is_empty() {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                "version",
                "Package version is required",
                None,
                Some("Add 'version: 0.1.0' to the package file."),
            ));
        } else if !Self::is_valid_version(&package.version) {
            result.add_issue(ValidationIssue::warning(
                ValidationErrorCategory::InvalidValue,
                "version",
                "Package version should follow semantic versioning",
                None,
                Some("Consider using a semantic version like '1.0.0'."),
            ));
        }

        // Check environments
        if package.environments.is_empty() {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                "environments",
                "At least one environment must be defined",
                None,
                Some("Add an 'environments' section with at least one environment."),
            ));
        }
    }

    /// Validate environments section
    fn validate_environments(&self, package: &Package, result: &mut ValidationResult) {
        if package.environments.is_empty() {
            return; // Already captured in required fields check
        }

        // Check if current environment is configured
        let current_env = &self.config.environment;
        if !current_env.is_empty() && !package.environments.contains_key(current_env) {
            result.add_issue(ValidationIssue::warning(
                ValidationErrorCategory::Environment,
                "environments",
                &format!("Current environment '{}' is not configured", current_env),
                None,
                Some(&format!(
                    "Add an environment section for '{}' if needed for this environment.",
                    current_env
                )),
            ));
        }

        // Validate each environment
        for (env_name, env_config) in &package.environments {
            self.validate_environment_config(env_name, env_config, result);
        }
    }

    /// Validate a specific environment configuration
    fn validate_environment_config(
        &self,
        env_name: &str,
        env_config: &EnvironmentConfig,
        result: &mut ValidationResult,
    ) {
        // Check install command
        if env_config.install.is_empty() {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::RequiredField,
                &format!("environments.{}.install", env_name),
                "Install command is required",
                None,
                Some("Add an install command like 'brew install package-name'."),
            ));
        } else {
            // Validate install command syntax
            self.validate_command_syntax(
                &env_config.install,
                &format!("environments.{}.install", env_name),
                result,
            );
        }

        // Check check command syntax if present
        if let Some(check_cmd) = &env_config.check {
            self.validate_command_syntax(
                check_cmd,
                &format!("environments.{}.check", env_name),
                result,
            );
        }

        // Validate dependencies (just simple checks for now)
        for (i, dep) in env_config.dependencies.iter().enumerate() {
            if dep.is_empty() {
                result.add_issue(ValidationIssue::error(
                    ValidationErrorCategory::InvalidValue,
                    &format!("environments.{}.dependencies[{}]", env_name, i),
                    "Dependency name cannot be empty",
                    None,
                    Some("Remove the empty dependency or provide a valid name."),
                ));
            }
        }
    }

    /// Validate command syntax for basic shell errors
    fn validate_command_syntax(
        &self,
        command: &str,
        field_name: &str,
        result: &mut ValidationResult,
    ) {
        // Check for unmatched quotes
        let mut in_single_quotes = false;
        let mut in_double_quotes = false;

        for c in command.chars() {
            match c {
                '\'' if !in_double_quotes => in_single_quotes = !in_single_quotes,
                '"' if !in_single_quotes => in_double_quotes = !in_double_quotes,
                _ => {}
            }
        }

        if in_single_quotes {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Unmatched single quote in command",
                None,
                Some("Add a closing single quote (') to the command."),
            ));
        }

        if in_double_quotes {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Unmatched double quote in command",
                None,
                Some("Add a closing double quote (\") to the command."),
            ));
        }

        // Check for invalid pipe usage
        if command.contains("| |") {
            result.add_issue(ValidationIssue::error(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Invalid pipe usage in command",
                None,
                Some("Remove duplicate pipe symbols."),
            ));
        }

        // Check for invalid redirections
        for redirect in &[">", ">>", "<"] {
            if command.contains(&format!("{} ", redirect))
                && !command.contains(&format!("{} /", redirect))
                && !command.contains(&format!("{} ~/", redirect))
            {
                result.add_issue(ValidationIssue::warning(
                    ValidationErrorCategory::CommandSyntax,
                    field_name,
                    &format!("Potential invalid redirection with {}", redirect),
                    None,
                    Some("Ensure the redirection path is valid and absolute."),
                ));
            }
        }

        // Check for command injection risks with backticks
        if command.contains('`') {
            result.add_issue(ValidationIssue::warning(
                ValidationErrorCategory::CommandSyntax,
                field_name,
                "Contains command substitution with backticks",
                None,
                Some("Consider using $() for command substitution instead of backticks."),
            ));
        }
    }

    /// Validate URL format
    fn validate_url(&self, url_str: &str, field_name: &str, result: &mut ValidationResult) {
        match Url::parse(url_str) {
            Ok(url) => {
                // Check scheme
                if url.scheme() != "http" && url.scheme() != "https" {
                    result.add_issue(ValidationIssue::warning(
                        ValidationErrorCategory::UrlFormat,
                        field_name,
                        &format!(
                            "URL should use http or https scheme, found: {}",
                            url.scheme()
                        ),
                        None,
                        Some("Use https:// prefix for the URL."),
                    ));
                }
            }
            Err(err) => {
                result.add_issue(ValidationIssue::error(
                    ValidationErrorCategory::UrlFormat,
                    field_name,
                    &format!("Invalid URL format: {}", err),
                    None,
                    Some("Provide a valid URL with http:// or https:// prefix."),
                ));
            }
        }
    }

    /// Check if a string is a valid package name
    fn is_valid_package_name(name: &str) -> bool {
        // Package names should only contain alphanumeric chars, hyphens, and underscores
        !name.is_empty()
            && name
                .chars()
                .all(|c| c.is_alphanumeric() || c == '-' || c == '_')
    }

    /// Check if a string is a valid version (basic semantic versioning check)
    fn is_valid_version(version: &str) -> bool {
        // Simple check for semver format: major.minor.patch
        let semver_regex = regex::Regex::new(r"^\d+\.\d+\.\d+").unwrap();
        semver_regex.is_match(version)
    }
}

/// Formats validation results for display, with optional color
pub fn format_validation_result(
    result: &ValidationResult,
    use_colors: bool,
    verbose: bool,
) -> String {
    let mut output = String::new();

    if result.is_valid() {
        let status = if use_colors {
            style("✓").green().to_string()
        } else {
            "✓".to_string()
        };

        let package_name = if use_colors {
            style(&result.package_name).magenta().bold().to_string()
        } else {
            result.package_name.clone()
        };

        output.push_str(&format!("{} Package '{}' is valid\n", status, package_name));

        // Add warnings if any
        let warnings = result.warnings();
        if !warnings.is_empty() {
            let warning_header = if use_colors {
                style("Warnings:").yellow().bold().to_string()
            } else {
                "Warnings:".to_string()
            };

            output.push_str(&format!("\n{}\n", warning_header));

            for warning in warnings {
                let warn_prefix = if use_colors {
                    style("  ! ").yellow().to_string()
                } else {
                    "  ! ".to_string()
                };

                output.push_str(&format!("{}{}\n", warn_prefix, warning.message));

                if let Some(suggestion) = &warning.suggestion {
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
    } else {
        let status = if use_colors {
            style("✗").red().bold().to_string()
        } else {
            "✗".to_string()
        };

        let package_name = if use_colors {
            style(&result.package_name).magenta().bold().to_string()
        } else {
            result.package_name.clone()
        };

        output.push_str(&format!(
            "{} Validation failed for package: {}\n",
            status, package_name
        ));

        // Group errors by category
        let mut errors_by_category = HashMap::new();
        for error in result.errors() {
            errors_by_category
                .entry(error.category)
                .or_insert_with(Vec::new)
                .push(error);
        }

        // Print required field errors first
        if let Some(errors) = errors_by_category.get(&ValidationErrorCategory::RequiredField) {
            let header = if use_colors {
                style("\nRequired field errors:").red().bold().to_string()
            } else {
                "\nRequired field errors:".to_string()
            };

            output.push_str(&header);
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

        // Then command syntax errors
        if let Some(errors) = errors_by_category.get(&ValidationErrorCategory::CommandSyntax) {
            let header = if use_colors {
                style("\nCommand syntax errors:").red().bold().to_string()
            } else {
                "\nCommand syntax errors:".to_string()
            };

            output.push_str(&header);
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

        // Then URL format errors
        if let Some(errors) = errors_by_category.get(&ValidationErrorCategory::UrlFormat) {
            let header = if use_colors {
                style("\nURL format errors:").red().bold().to_string()
            } else {
                "\nURL format errors:".to_string()
            };

            output.push_str(&header);
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

        // Then other validation errors
        for (category, errors) in &errors_by_category {
            if *category != ValidationErrorCategory::RequiredField
                && *category != ValidationErrorCategory::CommandSyntax
                && *category != ValidationErrorCategory::UrlFormat
            {
                let header = if use_colors {
                    style(format!("\n{:?} errors:", category))
                        .red()
                        .bold()
                        .to_string()
                } else {
                    format!("\n{:?} errors:", category)
                };

                output.push_str(&header);
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
        }

        // Show file path
        if let Some(path) = &result.package_path {
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
    }

    if verbose {
        // Add additional details like file structure, YAML parsing details, etc.
        if let Some(path) = &result.package_path {
            output.push_str("\nPackage file details:\n");
            output.push_str(&format!("  Path: {}\n", path.display()));

            let metadata = path.metadata().expect("metadata call failed");
            output.push_str(&format!(
                "  Permissions: {:o}\n",
                metadata.permissions().mode()
            ));

            let now = Zoned::now();
            let printer = SpanPrinter::new();
            {
                let created = Zoned::try_from(metadata.created().unwrap()).unwrap();
                let ago = now
                    .duration_since(&created)
                    .round(Unit::Second)
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  Created: {:?} ({} ago)\n",
                    &created.round(Unit::Second),
                    printer.duration_to_string(&ago)
                ));
            }
            {
                let modified = Zoned::try_from(metadata.modified().unwrap()).unwrap();
                let ago = now
                    .duration_since(&modified)
                    .round(Unit::Second)
                    .unwrap_or_default();
                output.push_str(&format!(
                    "  Modified: {:?} ({} ago)\n",
                    &modified.round(Unit::Second),
                    printer.duration_to_string(&ago)
                ));
            }
        }

        if let Some(package) = &result.package {
            output.push_str("\nPackage structure details:\n");
            output.push_str(&format!("  Version: {}\n", &package.version));
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
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::ConfigBuilder;
    use crate::filesystem::mock::MockFileSystem;
    use std::path::Path;

    // Helper function to create a test environment
    fn setup_test_environment() -> (MockFileSystem, Config) {
        let fs = MockFileSystem::default();
        let config = ConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/packages")
            .build();

        // Add the package directory to the filesystem
        fs.add_existing_path(Path::new("/test/packages"));

        (fs, config)
    }

    // Helper to create a valid package YAML
    fn create_valid_package_yaml() -> String {
        r#"
name: test-package
version: 1.0.0
homepage: https://example.com
description: A test package
environments:
  test-env:
    install: brew install test-package
    check: which test-package
  prod-env:
    install: apt-get install test-package
    dependencies:
      - dependency1
      - dependency2
"#
        .to_string()
    }

    #[test]
    fn test_validate_valid_package() {
        let (fs, config) = setup_test_environment();

        // Add a valid package file
        let yaml = create_valid_package_yaml();
        fs.add_file(Path::new("/test/packages/test-package.yaml"), &yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator.validate_package("test-package").unwrap();

        assert!(result.is_valid());
        assert_eq!(result.issues.len(), 0);
    }

    #[test]
    fn test_validate_missing_required_fields() {
        let (fs, config) = setup_test_environment();

        // Add an invalid package file with missing fields
        // Using valid YAML with empty fields rather than missing fields
        let yaml = r#"
name: ""
version: ""
environments:
  test-env:
    install: brew install test-package
"#;
        fs.add_file(Path::new("/test/packages/incomplete.yaml"), yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator
            .validate_package_file(Path::new("/test/packages/incomplete.yaml"))
            .unwrap();

        assert!(!result.is_valid());

        // Should have errors for missing name and version
        let required_field_errors =
            result.issues_by_category(&ValidationErrorCategory::RequiredField);
        assert_eq!(required_field_errors.len(), 2);

        // Check specific error messages
        let name_error = required_field_errors.iter().find(|e| e.field == "name");
        assert!(name_error.is_some());

        let version_error = required_field_errors.iter().find(|e| e.field == "version");
        assert!(version_error.is_some());
    }

    #[test]
    fn test_validate_invalid_url() {
        let (fs, config) = setup_test_environment();

        // Add a package with invalid URL
        let yaml = r#"
name: test-package
version: 1.0.0
homepage: not-a-valid-url
environments:
  test-env:
    install: brew install test-package
"#;
        fs.add_file(Path::new("/test/packages/invalid-url.yaml"), yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator
            .validate_package_file(Path::new("/test/packages/invalid-url.yaml"))
            .unwrap();

        // Should have a URL format error
        let url_errors = result.issues_by_category(&ValidationErrorCategory::UrlFormat);
        assert_eq!(url_errors.len(), 1);
        assert_eq!(url_errors[0].field, "homepage");
    }

    #[test]
    fn test_validate_command_syntax() {
        let (fs, config) = setup_test_environment();

        // Add a package with command syntax errors
        let yaml = r#"
name: test-package
version: 1.0.0
environments:
  test-env:
    install: brew install test-package "with unmatched quote
    check: echo "hello | | invalid pipes"
"#;
        fs.add_file(Path::new("/test/packages/bad-commands.yaml"), yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator
            .validate_package_file(Path::new("/test/packages/bad-commands.yaml"))
            .unwrap();

        // Should have command syntax errors
        let cmd_errors = result.issues_by_category(&ValidationErrorCategory::CommandSyntax);
        assert!(cmd_errors.len() >= 2); // At least the quote and pipe errors

        // Check specific error types
        assert!(cmd_errors
            .iter()
            .any(|e| e.message.contains("Unmatched double quote")));
        assert!(cmd_errors
            .iter()
            .any(|e| e.message.contains("Invalid pipe usage")));
    }

    #[test]
    fn test_validate_version_format() {
        let (fs, config) = setup_test_environment();

        // Add a package with non-semver version
        let yaml = r#"
name: test-package
version: abc
environments:
  test-env:
    install: brew install test-package
"#;
        fs.add_file(Path::new("/test/packages/bad-version.yaml"), yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator
            .validate_package_file(Path::new("/test/packages/bad-version.yaml"))
            .unwrap();

        // Should have a version format warning (not error)
        let version_issues = result.issues_by_category(&ValidationErrorCategory::InvalidValue);
        assert_eq!(version_issues.len(), 1);
        assert!(version_issues[0].is_warning);
        assert_eq!(version_issues[0].field, "version");
    }

    #[test]
    fn test_validate_missing_environment() {
        let (fs, config) = setup_test_environment();

        // Add a package without the current environment
        let yaml = r#"
name: test-package
version: 1.0.0
environments:
  other-env:  # Not the current environment (test-env)
    install: brew install test-package
"#;
        fs.add_file(Path::new("/test/packages/missing-env.yaml"), yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator
            .validate_package_file(Path::new("/test/packages/missing-env.yaml"))
            .unwrap();

        // Should have an environment warning
        let env_issues = result.issues_by_category(&ValidationErrorCategory::Environment);
        assert_eq!(env_issues.len(), 1);
        assert!(env_issues[0].is_warning);
        assert!(env_issues[0].message.contains("'test-env'"));
    }

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

        // Format the result
        let formatted = format_validation_result(&result, false, false);

        // Check the output contains expected content
        assert!(formatted.contains("Validation failed"));
        assert!(formatted.contains("Package name is required"));
        assert!(formatted.contains("Add 'name: your-package-name'"));
    }

    #[test]
    fn test_package_not_found() {
        let (fs, config) = setup_test_environment();

        let validator = PackageValidator::new(&fs, &config);
        let result = validator.validate_package("nonexistent");

        assert!(matches!(
            result,
            Err(PackageValidatorError::PackageNotFound(_))
        ));
    }

    #[test]
    fn test_multiple_packages_found() {
        let (fs, config) = setup_test_environment();

        // Add two files for the same package
        let yaml = create_valid_package_yaml();
        fs.add_file(Path::new("/test/packages/duplicate.yaml"), &yaml);
        fs.add_file(Path::new("/test/packages/duplicate.yml"), &yaml);

        let validator = PackageValidator::new(&fs, &config);
        let result = validator.validate_package("duplicate");

        assert!(matches!(
            result,
            Err(PackageValidatorError::MultiplePackagesFound(_))
        ));
    }
}
