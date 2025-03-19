// src/ports/command.rs
// Command execution port (interface)
use std::time::Duration;

use async_trait::async_trait;
use thiserror::Error;

pub enum OutputChunk {
    Stdout(String),
    Stderr(String),
}

/// Port for command execution
#[cfg_attr(test, mockall::automock)]
#[async_trait]
pub trait CommandRunner: Send + Sync {
    /// Execute a command and return its output
    async fn execute(&self, command: &str) -> Result<CommandOutput, CommandError>;

    /// Execute a command with a timeout and return its output
    async fn execute_with_timeout(
        &self,
        command: &str,
        timeout: Duration,
    ) -> Result<CommandOutput, CommandError>;

    async fn execute_streaming<F>(
        &self,
        command: &str,
        timeout: Duration,
        output_callback: F,
    ) -> Result<CommandOutput, CommandError>
    where
        F: FnMut(OutputChunk) + Send + 'static;

    /// Check if a command is available in the current environment
    async fn is_command_available(&self, command: &str) -> bool;
}

/// Result of executing a command
#[derive(Debug, Clone, PartialEq, Default)]
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

    #[error("IO Error: {0}")]
    IoError(String),
}

impl From<std::io::Error> for CommandError {
    fn from(value: std::io::Error) -> Self {
        Self::IoError(value.to_string())
    }
}

#[cfg(test)]
impl MockCommandRunner {
    pub(crate) fn mock_is_command_available(&mut self, command: &str, result: bool) {
        let command = command.to_string();

        self.expect_is_command_available()
            .with(mockall::predicate::eq(command))
            .returning(move |_| result);
    }

    pub(crate) fn mock_execute_streaming_ok(
        &mut self,
        command: &str,
        timeout: Duration,
        output: CommandOutput,
    ) {
        let cmd = command.to_string();

        self.expect_execute_streaming()
            .with(
                mockall::predicate::eq(cmd),
                mockall::predicate::eq(timeout),
                mockall::predicate::always(),
            )
            .return_const(Ok(output));
    }

    pub(crate) fn mock_execute_streaming_success_0(
        &mut self,
        command: &str,
        timeout: u64,
        stdout: &str,
    ) {
        self.mock_execute_streaming_ok(
            command,
            Duration::from_secs(timeout),
            CommandOutput {
                stdout: stdout.to_string(),
                stderr: String::new(),
                status: 0,
                success: true,
                duration: Duration::from_millis(100),
            },
        );
    }

    pub(crate) fn mock_execute_streaming_success_1(
        &mut self,
        command: &str,
        timeout: u64,
        stderr: &str,
    ) {
        self.mock_execute_streaming_ok(
            command,
            Duration::from_secs(timeout),
            CommandOutput {
                stdout: String::new(),
                stderr: stderr.to_string(),
                status: 1,
                success: false,
                duration: Duration::from_millis(100),
            },
        );
    }
}
