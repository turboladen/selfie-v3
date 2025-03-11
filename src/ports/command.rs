// src/ports/command.rs
// Command execution port (interface)

use std::time::Duration;
use thiserror::Error;

/// Result of executing a command
#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutput {
    /// Standard output from the command
    pub(crate) stdout: String,

    /// Standard error from the command
    pub(crate) stderr: String,

    /// Exit status code
    pub(crate) status: i32,

    /// Whether the command was successful (status code 0)
    pub(crate) success: bool,

    /// How long the command took to execute
    pub(crate) duration: Duration,
}

/// Errors that can occur during command execution
#[derive(Error, Debug, Clone)]
pub enum CommandError {
    #[error("Command execution failed: {0}")]
    ExecutionError(String),

    #[error("Command timed out after {0:?}")]
    Timeout(Duration),

    #[error("IO error: {0}")]
    IoError(String),
}

/// Port for command execution
#[cfg_attr(test, mockall::automock)]
pub trait CommandRunner {
    /// Execute a command and return its output
    fn execute(&self, command: &str) -> Result<CommandOutput, CommandError>;

    /// Execute a command with a timeout and return its output
    fn execute_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError>;

    /// Check if a command is available in the current environment
    fn is_command_available(&self, command: &str) -> bool;
}

#[cfg(test)]
impl MockCommandRunner {
    pub(crate) fn mock_is_command_available(&mut self, command: &str, result: bool) {
        let command = command.to_string();

        self.expect_is_command_available()
            .with(mockall::predicate::eq(command))
            .returning(move |_| result);
    }
}

// Helper functions to configure the mock command runner
#[cfg(test)]
pub(crate) trait MockCommandRunnerExt {
    fn add_response(&mut self, command: &str, response: Result<CommandOutput, CommandError>);
    fn add_command(&mut self, command: &str);
    fn success_response(&mut self, command: &str, stdout: &str);
    fn error_response(&mut self, command: &str, stderr: &str, status: i32);
    fn timeout_response(&mut self, command: &str, timeout: Duration);
}

#[cfg(test)]
impl MockCommandRunnerExt for MockCommandRunner {
    fn add_response(&mut self, command: &str, response: Result<CommandOutput, CommandError>) {
        let cmd = command.to_string();
        let resp = response.clone();

        // Set up execute to return the response for this command
        self.expect_execute()
            .with(mockall::predicate::eq(cmd.clone()))
            .returning(move |_| resp.clone());

        // Set up execute_with_timeout to also return the same response
        let resp2 = response.clone();
        let cmd2 = cmd.clone();
        self.expect_execute_with_timeout()
            .with(mockall::predicate::eq(cmd2), mockall::predicate::always())
            .returning(move |_, _| resp2.clone());
    }

    fn add_command(&mut self, command: &str) {
        let cmd = command.to_string();

        // Set up is_command_available to return true for this command
        self.expect_is_command_available()
            .with(mockall::predicate::eq(cmd))
            .returning(|_| true);
    }

    fn success_response(&mut self, command: &str, stdout: &str) {
        let output = CommandOutput {
            stdout: stdout.to_string(),
            stderr: String::new(),
            status: 0,
            success: true,
            duration: Duration::from_millis(100),
        };

        self.add_response(command, Ok(output));
    }

    fn error_response(&mut self, command: &str, stderr: &str, status: i32) {
        let output = CommandOutput {
            stdout: String::new(),
            stderr: stderr.to_string(),
            status,
            success: false,
            duration: Duration::from_millis(100),
        };

        self.add_response(command, Ok(output));
    }

    fn timeout_response(&mut self, command: &str, timeout: Duration) {
        self.add_response(command, Err(CommandError::Timeout(timeout)));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_command_runner_success() {
        let mut runner = MockCommandRunner::new();
        runner.success_response("echo hello", "hello");

        let output = runner.execute("echo hello").unwrap();
        assert_eq!(output.stdout, "hello");
        assert_eq!(output.stderr, "");
        assert_eq!(output.status, 0);
        assert!(output.success);
    }

    #[test]
    fn test_mock_command_runner_error() {
        let mut runner = MockCommandRunner::new();
        runner.error_response("invalid command", "command not found", 127);

        let output = runner.execute("invalid command").unwrap();
        assert_eq!(output.stdout, "");
        assert_eq!(output.stderr, "command not found");
        assert_eq!(output.status, 127);
        assert!(!output.success);
    }

    #[test]
    fn test_mock_command_runner_timeout() {
        let mut runner = MockCommandRunner::new();
        let timeout = Duration::from_secs(30);
        runner.timeout_response("slow command", timeout);

        let result = runner.execute("slow command");
        assert!(matches!(result, Err(CommandError::Timeout(_))));
        if let Err(CommandError::Timeout(duration)) = result {
            assert_eq!(duration, timeout);
        }
    }

    #[test]
    fn test_mock_command_availability() {
        let mut runner = MockCommandRunner::new();
        runner.mock_is_command_available("available", true);
        runner.mock_is_command_available("not_available", false);

        assert!(runner.is_command_available("available"));
        assert!(!runner.is_command_available("not_available"));
    }
}
