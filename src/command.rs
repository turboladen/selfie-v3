// src/command.rs

use std::{
    collections::HashMap,
    process::{Command, Output, Stdio},
    time::{Duration, Instant},
};

use thiserror::Error;

#[derive(Debug, Clone, PartialEq)]
pub struct CommandOutput {
    pub stdout: String,
    pub stderr: String,
    pub status: i32,
    pub success: bool,
    pub duration: Duration,
}

#[derive(Error, Debug, Clone)] // Added Clone here
pub enum CommandError {
    #[error("Command execution failed: {0}")]
    ExecutionError(String),

    #[error("Command timed out after {0:?}")]
    Timeout(Duration),

    #[error("IO error: {0}")]
    IoError(String),

    #[error("Command interrupted: {0}")]
    InterruptedError(String),
}

pub trait CommandRunner {
    /// Execute a command and return its output.
    fn execute(&self, command: &str) -> Result<CommandOutput, CommandError>;

    /// Execute a command with a timeout and return its output.
    fn execute_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError>;

    /// Check if a command is available in the current environment
    fn is_command_available(&self, command: &str) -> bool;
}

#[derive(Clone)] // Added Clone here
pub struct ShellCommandRunner {
    shell: String,
    default_timeout: Duration,
    environment: HashMap<String, String>,
}

impl ShellCommandRunner {
    pub fn new(shell: &str, default_timeout: Duration) -> Self {
        Self {
            shell: shell.to_string(),
            default_timeout,
            environment: HashMap::new(),
        }
    }

    pub fn with_environment(mut self, env: HashMap<String, String>) -> Self {
        self.environment = env;
        self
    }

    pub fn with_env_var(mut self, key: &str, value: &str) -> Self {
        self.environment.insert(key.to_string(), value.to_string());
        self
    }

    fn process_output(&self, output: Output, duration: Duration) -> CommandOutput {
        let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        let status = output.status.code().unwrap_or(-1);
        let success = output.status.success();

        CommandOutput {
            stdout,
            stderr,
            status,
            success,
            duration,
        }
    }
}

impl CommandRunner for ShellCommandRunner {
    fn execute(&self, command: &str) -> Result<CommandOutput, CommandError> {
        self.execute_with_timeout(command, self.default_timeout)
    }

    // Update in ShellCommandRunner implementation
    fn execute_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError> {
        let start_time = Instant::now();

        let mut cmd = Command::new(&self.shell);
        cmd.arg("-c")
            .arg(command)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());

        // Add environment variables
        for (key, value) in &self.environment {
            cmd.env(key, value);
        }

        // Execute the command
        // Note: this is a simplified implementation and doesn't truly enforce timeouts
        // A more robust implementation would involve async processing or threading
        let output = cmd
            .output()
            .map_err(|e| CommandError::IoError(e.to_string()))?;
        let duration = start_time.elapsed();

        // Simple timeout check (after the fact)
        if duration > timeout {
            return Err(CommandError::Timeout(timeout));
        }

        Ok(self.process_output(output, duration))
    }

    fn is_command_available(&self, command: &str) -> bool {
        // Shell-agnostic way to check if a command exists
        let check_cmd = format!("command -v {} >/dev/null 2>&1", command);
        match self.execute(&check_cmd) {
            Ok(output) => output.success,
            Err(_) => false,
        }
    }
}

pub mod mock {
    use super::*;
    use std::cell::RefCell;
    use std::collections::{HashMap, HashSet};

    #[derive(Default)]
    pub struct MockCommandRunner {
        responses: RefCell<HashMap<String, Result<CommandOutput, CommandError>>>,
        available_commands: RefCell<HashSet<String>>,
    }

    // Make sure the MockCommandRunner implements Clone for the integration test
    impl Clone for MockCommandRunner {
        fn clone(&self) -> Self {
            MockCommandRunner {
                responses: self.responses.clone(),
                available_commands: self.available_commands.clone(),
            }
        }
    }

    impl MockCommandRunner {
        pub fn new() -> Self {
            Self::default()
        }

        pub fn add_response(&self, command: &str, response: Result<CommandOutput, CommandError>) {
            self.responses
                .borrow_mut()
                .insert(command.to_string(), response);
        }

        pub fn add_command(&self, command: &str) {
            self.available_commands
                .borrow_mut()
                .insert(command.to_string());
        }

        pub fn success_response(&self, command: &str, stdout: &str) {
            self.add_response(
                command,
                Ok(CommandOutput {
                    stdout: stdout.to_string(),
                    stderr: String::new(),
                    status: 0,
                    success: true,
                    duration: Duration::from_millis(100),
                }),
            );
        }

        pub fn error_response(&self, command: &str, stderr: &str, status: i32) {
            self.add_response(
                command,
                Ok(CommandOutput {
                    stdout: String::new(),
                    stderr: stderr.to_string(),
                    status,
                    success: false,
                    duration: Duration::from_millis(100),
                }),
            );
        }

        pub fn timeout_response(&self, command: &str, timeout: Duration) {
            self.add_response(command, Err(CommandError::Timeout(timeout)));
        }
    }

    impl CommandRunner for MockCommandRunner {
        fn execute(&self, command: &str) -> Result<CommandOutput, CommandError> {
            self.responses
                .borrow()
                .get(command)
                .cloned()
                .unwrap_or_else(|| {
                    Err(CommandError::ExecutionError(format!(
                        "No mock response for command: `{}`.\nAll commands:\n{:#?}",
                        command, self.responses
                    )))
                })
        }

        fn execute_with_timeout(
            &self,
            command: &str,
            _timeout: Duration,
        ) -> Result<CommandOutput, CommandError> {
            // For the mock, we'll ignore the timeout parameter and just return the pre-configured response
            self.execute(command)
        }

        fn is_command_available(&self, command: &str) -> bool {
            self.available_commands.borrow().contains(command)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mock_command_runner_success() {
        let runner = mock::MockCommandRunner::new();
        runner.success_response("echo hello", "hello");

        let output = runner.execute("echo hello").unwrap();
        assert_eq!(output.stdout, "hello");
        assert_eq!(output.stderr, "");
        assert_eq!(output.status, 0);
        assert!(output.success);
    }

    #[test]
    fn test_mock_command_runner_error() {
        let runner = mock::MockCommandRunner::new();
        runner.error_response("invalid command", "command not found", 127);

        let output = runner.execute("invalid command").unwrap();
        assert_eq!(output.stdout, "");
        assert_eq!(output.stderr, "command not found");
        assert_eq!(output.status, 127);
        assert!(!output.success);
    }

    #[test]
    fn test_mock_command_runner_timeout() {
        let runner = mock::MockCommandRunner::new();
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
        let runner = mock::MockCommandRunner::new();
        runner.add_command("available");

        assert!(runner.is_command_available("available"));
        assert!(!runner.is_command_available("not_available"));
    }

    // Add test for ShellCommandRunner when run in a test environment
    #[test]
    fn test_shell_command_runner_basic() {
        let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(10));

        // Test a basic echo command
        let result = runner.execute("echo hello");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.stdout.contains("hello"));
        assert!(output.success);

        // Test command failure
        let result = runner.execute("exit 1");
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.success);
        assert_eq!(output.status, 1);
    }

    #[test]
    fn test_command_availability() {
        let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(10));

        // "echo" should be available in most environments
        assert!(runner.is_command_available("echo"));

        // A random string should not be a valid command
        let random_cmd = "xyzabc123notarealcommand";
        assert!(!runner.is_command_available(random_cmd));
    }
}

