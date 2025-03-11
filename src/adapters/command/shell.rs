// src/adapters/command/shell.rs
// Shell command runner adapter implementation

use std::collections::HashMap;
use std::process::{Command, Output, Stdio};
use std::time::{Duration, Instant};

use crate::ports::command::{CommandError, CommandOutput, CommandRunner};

/// Shell command runner implementation
#[derive(Clone)]
pub struct ShellCommandRunner {
    /// Path to the shell executable
    shell: String,

    /// Default timeout for commands
    default_timeout: Duration,

    /// Environment variables to set for commands
    environment: HashMap<String, String>,
}

impl ShellCommandRunner {
    /// Create a new shell command runner
    pub fn new(shell: &str, default_timeout: Duration) -> Self {
        Self {
            shell: shell.to_string(),
            default_timeout,
            environment: HashMap::new(),
        }
    }

    /// Process command output into a CommandOutput
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
        // TODO: this is a simplified implementation and doesn't truly enforce timeouts
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

#[cfg(test)]
mod tests {
    use super::*;

    // These tests will actually run commands on the system
    // They could be skipped in CI environments if necessary
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

    // This test relies on timing and could be flaky
    // Consider skipping or adjusting in CI environments
    #[test]
    fn test_timeout() {
        let runner = ShellCommandRunner::new("/bin/sh", Duration::from_millis(100));

        // Command that should timeout (sleep for 1s)
        // Note: This is a simple test and may be flaky since timeouts aren't enforced
        // in a separate thread in our implementation
        let result = runner.execute_with_timeout("sleep 1", Duration::from_millis(10));
        assert!(matches!(result, Err(CommandError::Timeout(_))));
    }
}
