// src/services/command_validator.rs
// Shell command validation functionality for package validator
//
// This module provides enhanced command validation capabilities for the package
// validation service, making better use of the CommandRunner trait.

use thiserror::Error;

use crate::{
    domain::package::EnvironmentConfig,
    ports::command::{CommandError, CommandRunner},
};

/// Errors that can occur during command validation
#[derive(Error, Debug)]
pub enum CommandValidationError {
    #[error("Command execution error: {0}")]
    ExecutionError(#[from] CommandError),

    #[error("Shell not available: {0}")]
    ShellNotAvailable(String),

    #[error("Command not available: {0}")]
    CommandNotAvailable(String),

    #[error("Invalid command syntax: {0}")]
    InvalidSyntax(String),

    #[error("Invalid shell specified: {0}")]
    InvalidShell(String),
}

/// Result of a command validation
#[derive(Debug, Clone)]
pub struct CommandValidationResult {
    /// Whether the command is valid
    pub is_valid: bool,
    /// The command that was validated
    pub command: String,
    /// Error message (if any)
    pub error: Option<String>,
    /// Whether the command is available in the current environment
    pub is_available: bool,
    /// Is this a warning rather than an error
    pub is_warning: bool,
    /// The environment this validation applies to
    pub environment: String,
}

/// Service for validating shell commands in package definitions
pub struct CommandValidator<'a, R: CommandRunner> {
    runner: &'a R,
}

impl<'a, R: CommandRunner> CommandValidator<'a, R> {
    /// Create a new command validator
    pub fn new(runner: &'a R) -> Self {
        Self { runner }
    }

    /// Validate a command in a package environment configuration
    pub fn validate_environment_commands(
        &self,
        env_name: &str,
        env_config: &EnvironmentConfig,
    ) -> Vec<CommandValidationResult> {
        let mut results = Vec::new();

        // Always validate the install command
        let install_result = self.validate_command_syntax(env_name, &env_config.install);
        results.push(install_result);

        // Validate check command if present
        if let Some(check_cmd) = &env_config.check {
            let check_result = self.validate_command_syntax(env_name, check_cmd);
            results.push(check_result);
        }

        // If validation enabled AND we have a shell command, check if basic shell commands are available
        if let Some(command) = Self::extract_base_command(&env_config.install) {
            let avail_result = self.check_command_availability(env_name, command);
            results.push(avail_result);
        }

        results
    }

    /// Validate command syntax without executing
    fn validate_command_syntax(&self, env_name: &str, command: &str) -> CommandValidationResult {
        // Check for unmatched quotes
        let mut in_single_quotes = false;
        let mut in_double_quotes = false;
        let mut is_valid = true;
        let mut error = None;

        for c in command.chars() {
            match c {
                '\'' if !in_double_quotes => in_single_quotes = !in_single_quotes,
                '"' if !in_single_quotes => in_double_quotes = !in_double_quotes,
                _ => {}
            }
        }

        if in_single_quotes {
            is_valid = false;
            error = Some("Unmatched single quote in command".to_string());
        } else if in_double_quotes {
            is_valid = false;
            error = Some("Unmatched double quote in command".to_string());
        }

        // Check for invalid pipe usage
        if command.contains("| |") {
            is_valid = false;
            error = Some("Invalid pipe usage in command".to_string());
        }

        // Check for other common syntax issues
        if command.contains(" > ") && !command.contains("> /") && !command.contains("> ~/") {
            return CommandValidationResult {
                is_valid: true, // Not a critical error
                command: command.to_string(),
                error: Some("Potential unsafe redirection in command".to_string()),
                is_available: true, // Don't know yet
                is_warning: true,   // This is just a warning
                environment: env_name.to_string(),
            };
        }

        CommandValidationResult {
            is_valid,
            command: command.to_string(),
            error,
            is_available: true, // Don't know yet
            is_warning: false,
            environment: env_name.to_string(),
        }
    }

    /// Extract the base command from a command string
    pub(crate) fn extract_base_command(command: &str) -> Option<&str> {
        // Simple extraction of the first word before a space, pipe, etc.
        command.split_whitespace().next()
    }

    /// Check if a command is available in the current environment
    fn check_command_availability(&self, env_name: &str, command: &str) -> CommandValidationResult {
        let is_available = self.runner.is_command_available(command);

        // Generate more environment-aware message
        let error_message = if !is_available {
            Some(format!(
                "Command '{}' not found in environment '{}'. This may cause installation issues.",
                command, env_name
            ))
        } else {
            None
        };

        CommandValidationResult {
            is_valid: true, // The command itself could be valid even if not available
            command: command.to_string(),
            error: error_message,
            is_available,
            is_warning: true, // This is a warning rather than an error
            environment: env_name.to_string(),
        }
    }

    /// Check if the command might require sudo
    pub fn might_require_sudo(&self, command: &str) -> bool {
        let sudo_indicators = [
            "sudo ",
            "apt ",
            "apt-get ",
            "dnf ",
            "yum ",
            "pacman ",
            "zypper ",
            "systemctl ",
        ];

        sudo_indicators
            .iter()
            .any(|&indicator| command.contains(indicator))
    }

    /// Enhanced check for commands specific to particular environments
    pub fn is_command_recommended_for_env(&self, env_name: &str, command: &str) -> Option<String> {
        // Map of environment prefixes to recommended package managers
        let env_recommendations = [
            // macOS environments
            ("mac", vec!["brew", "port", "mas"]),
            ("darwin", vec!["brew", "port", "mas"]),
            // Linux environments
            ("ubuntu", vec!["apt", "apt-get", "dpkg"]),
            ("debian", vec!["apt", "apt-get", "dpkg"]),
            ("fedora", vec!["dnf", "yum", "rpm"]),
            ("rhel", vec!["dnf", "yum", "rpm"]),
            ("centos", vec!["dnf", "yum", "rpm"]),
            ("arch", vec!["pacman", "yay", "paru"]),
            ("opensuse", vec!["zypper", "rpm"]),
            // Windows environments
            ("windows", vec!["choco", "scoop", "winget"]),
        ];

        // Check if environment matches and command doesn't use recommended package manager
        let env_name_lower = env_name.to_lowercase();

        for (env_pattern, recommended_managers) in &env_recommendations {
            // Check if environment name contains the pattern
            if env_name_lower.contains(env_pattern) {
                // Extract the base command and check if it's in the recommended list
                if let Some(base_cmd) = Self::extract_base_command(command) {
                    if !recommended_managers.iter().any(|&mgr| base_cmd == mgr) {
                        return Some(format!(
                            "Command may not be optimal for '{}' environment. Consider using: {}",
                            env_name,
                            recommended_managers.join(", ")
                        ));
                    }
                }
            }
        }

        None
    }

    /// Check if the command uses backticks (often considered unsafe)
    pub fn uses_backticks(&self, command: &str) -> bool {
        command.contains('`')
    }

    /// Check if the command might download content from the internet
    pub fn might_download_content(&self, command: &str) -> bool {
        let download_indicators = [
            "curl ",
            "wget ",
            "fetch ",
            "git clone",
            "git pull",
            "npm install",
            "pip install",
        ];

        download_indicators
            .iter()
            .any(|&indicator| command.contains(indicator))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::command::MockCommandRunner;

    #[test]
    fn test_validate_command_syntax_valid() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        let result = validator.validate_command_syntax("test-env", "echo hello");
        assert!(result.is_valid);
        assert!(result.error.is_none());
        assert_eq!(result.environment, "test-env");
    }

    #[test]
    fn test_validate_command_syntax_unmatched_quote() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        let result = validator.validate_command_syntax("test-env", "echo 'hello");
        assert!(!result.is_valid);
        assert!(result.error.unwrap().contains("Unmatched single quote"));
        assert_eq!(result.environment, "test-env");
    }

    #[test]
    fn test_validate_command_syntax_invalid_pipe() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        let result = validator.validate_command_syntax("test-env", "echo hello | | grep world");
        assert!(!result.is_valid);
        assert!(result.error.unwrap().contains("Invalid pipe usage"));
        assert_eq!(result.environment, "test-env");
    }

    #[test]
    fn test_validate_command_syntax_unsafe_redirection() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        let result = validator.validate_command_syntax("test-env", "echo hello > output.txt");
        assert!(result.is_valid); // This is valid but generates a warning
        assert!(result.is_warning);
        assert!(result.error.unwrap().contains("unsafe redirection"));
        assert_eq!(result.environment, "test-env");
    }

    #[test]
    fn test_extract_base_command() {
        assert_eq!(
            CommandValidator::<MockCommandRunner>::extract_base_command("echo hello"),
            Some("echo")
        );
        assert_eq!(
            CommandValidator::<MockCommandRunner>::extract_base_command("brew install ripgrep"),
            Some("brew")
        );
        assert_eq!(
            CommandValidator::<MockCommandRunner>::extract_base_command(
                "  apt-get install -y git  "
            ),
            Some("apt-get")
        );
    }

    #[test]
    fn test_check_command_availability() {
        let mut runner = MockCommandRunner::new();
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("echo"))
            .returning(|_| true);
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("nonexistent"))
            .returning(|_| false);

        let validator = CommandValidator::new(&runner);

        let result = validator.check_command_availability("mac-env", "echo");
        assert!(result.is_valid);
        assert!(result.is_available);
        assert!(result.error.is_none());
        assert_eq!(result.environment, "mac-env");

        let result = validator.check_command_availability("mac-env", "nonexistent");
        assert!(result.is_valid); // The command syntax is valid even if not available
        assert!(!result.is_available);
        assert!(result.error.unwrap().contains("not found"));
        assert_eq!(result.environment, "mac-env");
    }

    #[test]
    fn test_might_require_sudo() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        assert!(validator.might_require_sudo("sudo apt install git"));
        assert!(validator.might_require_sudo("apt install git"));
        assert!(validator.might_require_sudo("pacman -S git"));
        assert!(!validator.might_require_sudo("echo hello"));
        assert!(!validator.might_require_sudo("brew install git"));
    }

    #[test]
    fn test_uses_backticks() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        assert!(validator.uses_backticks("echo `date`"));
        assert!(!validator.uses_backticks("echo $(date)"));
    }

    #[test]
    fn test_might_download_content() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        assert!(validator.might_download_content("curl -O https://example.com/file"));
        assert!(validator.might_download_content("wget https://example.com/file"));
        assert!(validator.might_download_content("git clone https://github.com/user/repo"));
        assert!(validator.might_download_content("npm install express"));
        assert!(!validator.might_download_content("echo hello"));
    }

    #[test]
    fn test_validate_environment_commands() {
        let mut runner = MockCommandRunner::new();
        runner
            .expect_is_command_available()
            .with(mockall::predicate::eq("brew"))
            .returning(|_| true);

        let validator = CommandValidator::new(&runner);

        // Create a test environment config
        let env_config = EnvironmentConfig {
            install: "brew install ripgrep".to_string(),
            check: Some("which rg".to_string()),
            dependencies: vec![],
        };

        let results = validator.validate_environment_commands("mac-env", &env_config);

        // Should have 3 results: install syntax, check syntax, and command availability
        assert_eq!(results.len(), 3);

        // All should be valid
        assert!(results.iter().all(|r| r.is_valid));

        // The first two are syntax checks
        assert_eq!(results[0].command, "brew install ripgrep");
        assert_eq!(results[1].command, "which rg");
        assert_eq!(results[0].environment, "mac-env");
        assert_eq!(results[1].environment, "mac-env");

        // The third is availability check
        assert_eq!(results[2].command, "brew");
        assert!(results[2].is_available);
        assert_eq!(results[2].environment, "mac-env");
    }

    #[test]
    fn test_is_command_recommended_for_env() {
        let runner = MockCommandRunner::new();
        let validator = CommandValidator::new(&runner);

        // Test macOS environment recommendations
        let mac_result = validator.is_command_recommended_for_env("mac-env", "apt install package");
        assert!(mac_result.is_some());
        assert!(mac_result.unwrap().contains("brew"));

        // Test macOS environment with recommended command
        let mac_good = validator.is_command_recommended_for_env("mac-env", "brew install package");
        assert!(mac_good.is_none());

        // Test Ubuntu environment
        let ubuntu_result =
            validator.is_command_recommended_for_env("ubuntu-env", "brew install package");
        assert!(ubuntu_result.is_some());
        assert!(ubuntu_result.unwrap().contains("apt"));

        // Test Arch environment
        let arch_result =
            validator.is_command_recommended_for_env("arch-linux", "apt install package");
        assert!(arch_result.is_some());
        assert!(arch_result.unwrap().contains("pacman"));

        // Test Windows environment
        let windows_result =
            validator.is_command_recommended_for_env("windows-env", "apt install package");
        assert!(windows_result.is_some());
        assert!(windows_result.unwrap().contains("choco"));
    }
}
