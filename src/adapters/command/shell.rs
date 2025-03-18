// src/adapters/command/shell.rs
// Shell command runner adapter implementation

use std::collections::HashMap;
use std::process::{Output, Stdio};
use std::time::{Duration, Instant};

use async_trait::async_trait;
use futures::TryFutureExt;
use tokio::io::AsyncReadExt;
use tokio::process::Command;

use crate::ports::command::{CommandError, CommandOutput, CommandRunner, OutputChunk};

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

#[async_trait]
impl CommandRunner for ShellCommandRunner {
    async fn execute(&self, command: &str) -> Result<CommandOutput, CommandError> {
        self.execute_with_timeout(command, self.default_timeout)
            .await
    }

    async fn execute_with_timeout(
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

        let duration = start_time.elapsed();

        // Execute the command within the context of a timeout
        let output = tokio::time::timeout(timeout, cmd.output().map_err(CommandError::from))
            .await
            .map_err(|_| CommandError::Timeout(timeout))??;

        Ok(self.process_output(output, duration))
    }

    async fn is_command_available(&self, command: &str) -> bool {
        // Shell-agnostic way to check if a command exists
        let check_cmd = format!("command -v {} >/dev/null 2>&1", command);
        match self.execute(&check_cmd).await {
            Ok(output) => output.success,
            Err(_) => false,
        }
    }

    async fn execute_streaming<F>(
        &self,
        command: &str,
        timeout: Duration,
        mut output_callback: F,
    ) -> Result<CommandOutput, CommandError>
    where
        F: FnMut(OutputChunk) + Send + 'static,
    {
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

        // Spawn the command
        let mut child = cmd.spawn().map_err(CommandError::from)?;

        // Get stdout and stderr
        let mut stdout =
            tokio::io::BufReader::new(child.stdout.take().expect("Failed to get stdout"));
        let mut stderr =
            tokio::io::BufReader::new(child.stderr.take().expect("Failed to get stderr"));

        // Collect full output
        let mut full_stdout = String::new();
        let mut full_stderr = String::new();

        // Track partial lines
        let mut stdout_partial = String::new();
        let mut stderr_partial = String::new();

        // Create buffers
        let mut stdout_buf = [0u8; 1024];
        let mut stderr_buf = [0u8; 1024];

        // Track if stdout/stderr are done
        let mut stdout_done = false;
        let mut stderr_done = false;

        // Create the timeout future
        let timeout_future = tokio::time::sleep(timeout);
        tokio::pin!(timeout_future);

        // Main read loop
        loop {
            tokio::select! {
                // Check for timeout
                _ = &mut timeout_future => {
                    let _ = child.kill().await;
                    return Err(CommandError::Timeout(timeout));
                }

                // Read from stdout
                result = stdout.read(&mut stdout_buf), if !stdout_done => {
                    match result {
                        Ok(0) => { stdout_done = true; }
                        Ok(n) => {
                            let data = String::from_utf8_lossy(&stdout_buf[..n]).to_string();
                            full_stdout.push_str(&data);

                            // Process line by line
                            for (i, chunk) in data.split('\n').enumerate() {
                                if i == 0 && !chunk.is_empty() {
                                    // First chunk - append to partial line
                                    stdout_partial.push_str(chunk);
                                    if data.contains('\n') {
                                        // Line is complete
                                        output_callback(OutputChunk::Stdout(stdout_partial.clone()));
                                        stdout_partial.clear();
                                    } else {
                                        // Still a partial line
                                        output_callback(OutputChunk::StdoutPartial(stdout_partial.clone()));
                                    }
                                } else if i > 0 {
                                    // Middle/end chunks
                                    if !chunk.is_empty() || i < data.split('\n').count() - 1 {
                                        stdout_partial.push_str(chunk);
                                        if i < data.split('\n').count() - 1 || data.ends_with('\n') {
                                            // Complete line
                                            output_callback(OutputChunk::Stdout(stdout_partial.clone()));
                                            stdout_partial.clear();
                                        } else {
                                            // Partial line
                                            output_callback(OutputChunk::StdoutPartial(stdout_partial.clone()));
                                        }
                                    }
                                }
                            }
                        },
                        Err(e) => return Err(CommandError::IoError(e.to_string())),
                    }
                }

                // Read from stderr (similar logic)
                result = stderr.read(&mut stderr_buf), if !stderr_done => {
                    // Same logic as stdout but with stderr callbacks
                    match result {
                        Ok(0) => { stderr_done = true; }
                        Ok(n) => {
                            let data = String::from_utf8_lossy(&stderr_buf[..n]).to_string();
                            full_stderr.push_str(&data);

                            // Similar line processing logic with stderr callbacks
                            for (i, chunk) in data.split('\n').enumerate() {
                                if i == 0 && !chunk.is_empty() {
                                    stderr_partial.push_str(chunk);
                                    if data.contains('\n') {
                                        output_callback(OutputChunk::Stderr(stderr_partial.clone()));
                                        stderr_partial.clear();
                                    } else {
                                        output_callback(OutputChunk::StderrPartial(stderr_partial.clone()));
                                    }
                                } else if i > 0 && (!chunk.is_empty() || i < data.split('\n').count() - 1) {
                                    stderr_partial.push_str(chunk);
                                    if i < data.split('\n').count() - 1 || data.ends_with('\n') {
                                        output_callback(OutputChunk::Stderr(stderr_partial.clone()));
                                        stderr_partial.clear();
                                    } else {
                                        output_callback(OutputChunk::StderrPartial(stderr_partial.clone()));
                                    }
                                }
                            }
                        },
                        Err(e) => return Err(CommandError::IoError(e.to_string())),
                    }
                }

                // Wait for the process to complete when streams are done
                status = child.wait(), if stdout_done && stderr_done => {
                    let status = status.map_err(CommandError::from)?;
                    let duration = start_time.elapsed();

                    return Ok(CommandOutput {
                        stdout: full_stdout,
                        stderr: full_stderr,
                        status: status.code().unwrap_or(-1),
                        success: status.success(),
                        duration,
                    });
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // These tests will actually run commands on the system
    // They could be skipped in CI environments if necessary
    #[tokio::test]
    async fn test_shell_command_runner_basic() {
        let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(10));

        // Test a basic echo command
        let result = runner.execute("echo hello").await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(output.stdout.contains("hello"));
        assert!(output.success);

        // Test command failure
        let result = runner.execute("exit 1").await;
        assert!(result.is_ok());
        let output = result.unwrap();
        assert!(!output.success);
        assert_eq!(output.status, 1);
    }

    #[tokio::test]
    async fn test_command_availability() {
        let runner = ShellCommandRunner::new("/bin/sh", Duration::from_secs(10));

        // "echo" should be available in most environments
        assert!(runner.is_command_available("echo").await);

        // A random string should not be a valid command
        let random_cmd = "xyzabc123notarealcommand";
        assert!(!runner.is_command_available(random_cmd).await);
    }

    // This test relies on timing and could be flaky
    // Consider skipping or adjusting in CI environments
    #[tokio::test]
    async fn test_timeout() {
        let runner = ShellCommandRunner::new("/bin/sh", Duration::from_millis(100));

        // Command that should timeout (sleep for 1s)
        // Note: This is a simple test and may be flaky since timeouts aren't enforced
        // in a separate thread in our implementation
        let result = runner
            .execute_with_timeout("sleep 1", Duration::from_millis(10))
            .await;
        assert!(matches!(result, Err(CommandError::Timeout(_))));
    }
}
