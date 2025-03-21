// src/domain/installation.rs
// Installation domain model using state machine pattern

use std::time::{Duration, Instant};

use thiserror::Error;

use crate::ports::command::{CommandError, CommandOutput, CommandRunner, OutputChunk};

use super::package::EnvironmentConfig;

/// Represents a package installation as a state machine
#[derive(Debug, Clone)]
pub(crate) enum Installation {
    NotStarted {
        env_config: EnvironmentConfig,
    },
    Checking {
        env_config: EnvironmentConfig,
        start_time: Instant,
    },
    NotAlreadyInstalled {
        env_config: EnvironmentConfig,
        start_time: Instant,
        check_duration: Duration,
    },
    AlreadyInstalled {
        env_config: EnvironmentConfig,
        start_time: Instant,
        check_duration: Duration,
    },
    Installing {
        env_config: EnvironmentConfig,
        start_time: Instant,
        check_duration: Duration,
    },
    Complete {
        env_config: EnvironmentConfig,
        start_time: Instant,
        duration: Duration,
        command_output: CommandOutput,
    },
    Failed {
        env_config: EnvironmentConfig,
        start_time: Instant,
        duration: Duration,
        error_message: String,
    },
    Skipped {
        env_config: EnvironmentConfig,
        start_time: Instant,
        duration: Duration,
        reason: String,
    },
}

impl Installation {
    /// Create a new installation in NotStarted state
    pub(crate) fn new(env_config: EnvironmentConfig) -> Self {
        Self::NotStarted { env_config }
    }

    /// Start the installation process
    pub(crate) fn start(self) -> Self {
        match self {
            Self::NotStarted { env_config } => Self::Checking {
                env_config,
                start_time: Instant::now(),
            },
            other => other, // No-op for other states
        }
    }

    /// Mark as not installed after check
    fn mark_not_already_installed(self) -> Self {
        match self {
            Self::Checking {
                env_config,
                start_time,
            } => Self::NotAlreadyInstalled {
                env_config,
                start_time,
                check_duration: start_time.elapsed(),
            },
            other => other,
        }
    }

    /// Mark as already installed after check
    fn mark_already_installed(self) -> Self {
        match self {
            Self::Checking {
                env_config,
                start_time,
            } => Self::AlreadyInstalled {
                env_config,
                start_time,
                check_duration: start_time.elapsed(),
            },
            other => other,
        }
    }

    /// Start installing
    fn start_installing(self) -> Self {
        match self {
            Self::NotAlreadyInstalled {
                env_config,
                start_time,
                check_duration,
            } => Self::Installing {
                env_config,
                start_time,
                check_duration,
            },
            other => other,
        }
    }

    /// Mark as complete
    fn complete(self, command_output: CommandOutput) -> Self {
        match self {
            Self::Installing {
                env_config,
                start_time,
                ..
            } => Self::Complete {
                env_config,
                start_time,
                duration: start_time.elapsed(),
                command_output,
            },
            other => other,
        }
    }

    /// Mark as failed
    fn fail(self, error_message: String) -> Self {
        match self {
            Self::Checking {
                env_config,
                start_time,
            } => Self::Failed {
                env_config,
                start_time,
                duration: start_time.elapsed(),
                error_message,
            },
            Self::Installing {
                env_config,
                start_time,
                ..
            } => Self::Failed {
                env_config,
                start_time,
                duration: start_time.elapsed(),
                error_message,
            },
            Self::NotAlreadyInstalled {
                env_config,
                start_time,
                ..
            } => Self::Failed {
                env_config,
                start_time,
                duration: start_time.elapsed(),
                error_message,
            },
            other => other,
        }
    }

    /// Mark as skipped
    fn skip(self, reason: String) -> Self {
        match self {
            Self::NotStarted { env_config } => {
                let start_time = Instant::now();
                Self::Skipped {
                    env_config,
                    start_time,
                    duration: Duration::from_secs(0),
                    reason,
                }
            }
            Self::Checking {
                env_config,
                start_time,
            } => Self::Skipped {
                env_config,
                start_time,
                duration: start_time.elapsed(),
                reason,
            },
            other => other,
        }
    }

    // Add streaming version of execute_check
    pub(crate) async fn execute_check<CR: CommandRunner, F>(
        self,
        runner: &CR,
        output_callback: F,
    ) -> Result<Self, InstallationError>
    where
        F: FnMut(OutputChunk) + Send + 'static,
    {
        match &self {
            Self::Checking { env_config, .. } => {
                // If there's no check command, assume not installed
                if env_config.check.is_none() {
                    return Ok(self.mark_not_already_installed());
                }

                let check_cmd = env_config.check.as_ref().unwrap();

                // Execute the check command with streaming
                match runner
                    .execute_streaming(check_cmd, Duration::from_secs(60), output_callback)
                    .await
                {
                    Ok(output) => {
                        if output.success {
                            Ok(self.mark_already_installed())
                        } else {
                            Ok(self.mark_not_already_installed())
                        }
                    }
                    Err(e) => {
                        let error_message = format!("Check command failed: {}", e);
                        Ok(self.fail(error_message))
                    }
                }
            }
            _ => Err(InstallationError::InvalidState(
                "Can only execute check from Checking state".to_string(),
            )),
        }
    }

    pub(crate) async fn execute_install<CR: CommandRunner, F>(
        self,
        runner: &CR,
        output_callback: F,
    ) -> Result<Self, InstallationError>
    where
        F: FnMut(OutputChunk) + Send + 'static,
    {
        match &self.clone() {
            Self::NotAlreadyInstalled { env_config, .. } => {
                let installing = self.start_installing();

                // Execute the install command with streaming
                match runner
                    .execute_streaming(
                        &env_config.install,
                        Duration::from_secs(600),
                        output_callback,
                    )
                    .await
                {
                    Ok(output) => {
                        if output.success {
                            Ok(installing.complete(output))
                        } else {
                            let error_msg =
                                format!("Install command failed with status {}", output.status);
                            Ok(installing.fail(error_msg))
                        }
                    }
                    Err(e) => {
                        let error_msg = format!("Install command error: {}", e);
                        Ok(installing.fail(error_msg))
                    }
                }
            }
            _ => Err(InstallationError::InvalidState(
                "Can only execute install from NotInstalled state".to_string(),
            )),
        }
    }

    /// Get the current state as InstallationStatus
    pub(crate) fn status(&self) -> InstallationStatus {
        match self {
            Self::NotStarted { .. } => InstallationStatus::NotStarted,
            Self::Checking { .. } => InstallationStatus::Checking,
            Self::NotAlreadyInstalled { .. } => InstallationStatus::NotInstalled,
            Self::AlreadyInstalled { .. } => InstallationStatus::AlreadyInstalled,
            Self::Installing { .. } => InstallationStatus::Installing,
            Self::Complete { .. } => InstallationStatus::Complete,
            Self::Failed { error_message, .. } => InstallationStatus::Failed(error_message.clone()),
            Self::Skipped { reason, .. } => InstallationStatus::Skipped(reason.clone()),
        }
    }

    /// Get the installation duration
    pub(crate) fn duration(&self) -> Option<Duration> {
        match self {
            Self::NotStarted { .. } => None,
            Self::Checking { start_time, .. } => Some(start_time.elapsed()),
            Self::NotAlreadyInstalled { start_time, .. } => Some(start_time.elapsed()),
            Self::AlreadyInstalled { start_time, .. } => Some(start_time.elapsed()),
            Self::Installing { start_time, .. } => Some(start_time.elapsed()),
            Self::Complete { duration, .. } => Some(*duration),
            Self::Failed { duration, .. } => Some(*duration),
            Self::Skipped { duration, .. } => Some(*duration),
        }
    }

    /// Convert to InstallationResult
    pub(crate) fn into_result(
        self,
        package_name: String,
    ) -> Result<InstallationReport, InstallationError> {
        match self {
            Self::AlreadyInstalled { check_duration, .. } => Ok(InstallationReport {
                package_name,
                status: InstallationStatus::AlreadyInstalled,
                duration: check_duration,
                command_output: None,
                dependencies: Vec::new(),
            }),
            Self::Complete {
                duration,
                command_output,
                ..
            } => Ok(InstallationReport {
                package_name,
                status: InstallationStatus::Complete,
                duration,
                command_output: Some(command_output),
                dependencies: Vec::new(),
            }),
            Self::Failed { error_message, .. } => {
                Err(InstallationError::InstallationFailed(error_message))
            }
            Self::Skipped {
                duration, reason, ..
            } => Ok(InstallationReport {
                package_name,
                status: InstallationStatus::Skipped(reason),
                duration,
                command_output: None,
                dependencies: Vec::new(),
            }),
            Self::NotStarted { .. } => Err(InstallationError::InvalidState(
                "Invalid state transition: NotStarted".to_string(),
            )),
            Self::NotAlreadyInstalled { .. } => Err(InstallationError::InvalidState(
                "Invalid state transition: NotInstalled".to_string(),
            )),
            Self::Checking { .. } => Err(InstallationError::InvalidState(
                "Invalid state transition: Checking".to_string(),
            )),
            Self::Installing { .. } => Err(InstallationError::InvalidState(
                "Invalid state transition: Installing".to_string(),
            )),
        }
    }
}

/// Represents the current status of a package installation
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum InstallationStatus {
    /// Installation has not yet started
    NotStarted,

    /// Currently checking if the package is already installed
    Checking,

    /// Package is not installed
    NotInstalled,

    /// Package is already installed
    AlreadyInstalled,

    /// Package is currently being installed
    Installing,

    /// Installation completed successfully
    Complete,

    /// Installation failed with an error message
    Failed(String),

    /// Installation was skipped for the given reason
    Skipped(String),
}

/// Errors that can occur during installation
#[derive(Error, Debug)]
pub(crate) enum InstallationError {
    #[error("Command execution error: {0}")]
    CommandError(#[from] CommandError),

    #[error("Installation failed: {0}")]
    InstallationFailed(String),

    #[error("Check command failed: {0}")]
    CheckFailed(String),

    #[error("Invalid state transition: {0}")]
    InvalidState(String),
}

/// Represents the result of an installation operation
#[derive(Debug)]
pub(crate) struct InstallationReport {
    /// Name of the installed package
    pub(crate) package_name: String,

    /// Final installation status
    pub(crate) status: InstallationStatus,

    /// How long the installation took
    pub(crate) duration: Duration,

    pub(crate) command_output: Option<CommandOutput>,

    /// Results of dependent package installations
    pub(crate) dependencies: Vec<InstallationReport>,
}

impl InstallationReport {
    /// Add dependencies to the installation result
    pub(crate) fn with_dependencies(mut self, dependencies: Vec<InstallationReport>) -> Self {
        self.dependencies = dependencies;
        self
    }

    /// Calculate the total duration including dependencies
    pub(crate) fn total_duration(&self) -> Duration {
        let mut total = self.duration;
        for dep in &self.dependencies {
            total += dep.duration;
        }
        total
    }

    /// Calculate the duration of dependency installations
    pub(crate) fn dependency_duration(&self) -> Duration {
        let mut total = Duration::from_secs(0);
        for dep in &self.dependencies {
            total += dep.total_duration();
        }
        total
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use crate::ports::command::MockCommandRunner;

    fn create_test_env_config() -> EnvironmentConfig {
        EnvironmentConfig {
            install: "test install".to_string(),
            check: Some("test check".to_string()),
            dependencies: Vec::new(),
        }
    }

    #[test]
    fn test_installation_state_transitions() {
        let env_config = create_test_env_config();

        // Initial state
        let installation = Installation::new(env_config.clone());
        assert!(matches!(installation, Installation::NotStarted { .. }));

        // Start
        let installation = installation.start();
        assert!(matches!(installation, Installation::Checking { .. }));

        // Mark not installed
        let installation = installation.mark_not_already_installed();
        assert!(matches!(
            installation,
            Installation::NotAlreadyInstalled { .. }
        ));

        // Start installing
        let installation = installation.start_installing();
        assert!(matches!(installation, Installation::Installing { .. }));

        // Complete
        let output = CommandOutput {
            stdout: "success".to_string(),
            stderr: String::new(),
            status: 0,
            success: true,
            duration: Duration::from_millis(200),
        };
        let installation = installation.complete(output);
        assert!(matches!(installation, Installation::Complete { .. }));
    }

    #[test]
    fn test_failed_state_transition() {
        let env_config = create_test_env_config();

        // Test failure from checking state
        let installation = Installation::new(env_config.clone()).start();
        let failed = installation.fail("Test error".to_string());
        assert!(matches!(failed, Installation::Failed { .. }));

        // Test failure from installing state
        let installation = Installation::new(env_config.clone())
            .start()
            .mark_not_already_installed()
            .start_installing();
        let failed = installation.fail("Install error".to_string());
        assert!(matches!(failed, Installation::Failed { .. }));
    }

    #[tokio::test]
    async fn test_execute_check_streaming_no_check_command() {
        let env_config = EnvironmentConfig {
            install: "test install".to_string(),
            check: None,
            dependencies: Vec::new(),
        };

        let installation = Installation::new(env_config).start();
        let runner = MockCommandRunner::new();

        // Use Arc<Mutex<Vec>> to share ownership between closure and test
        let streamed_outputs = Arc::new(Mutex::new(Vec::new()));
        let outputs_clone = streamed_outputs.clone();

        let result = installation
            .execute_check(&runner, move |chunk| {
                let mut outputs = outputs_clone.lock().unwrap();
                match chunk {
                    OutputChunk::Stdout(line) => outputs.push(format!("stdout: {}", line)),
                    OutputChunk::Stderr(line) => outputs.push(format!("stderr: {}", line)),
                }
            })
            .await;

        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(matches!(state, Installation::NotAlreadyInstalled { .. }));

        // No outputs expected since no check command
        assert!(streamed_outputs.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn test_execute_check_streaming_success() {
        let env_config = create_test_env_config();
        let installation = Installation::new(env_config).start();

        let mut runner = MockCommandRunner::new();

        // Add mock for execute_streaming
        let output = CommandOutput {
            stdout: "Package found\n".to_string(),
            stderr: String::new(),
            status: 0,
            success: true,
            duration: Duration::from_millis(100),
        };

        runner
            .expect_execute_streaming()
            .withf(|cmd, _, _| cmd == "test check")
            .returning(move |_, _, mut callback| {
                // Simulate streaming output
                callback(OutputChunk::Stdout("Package found\n".to_string()));
                Ok(output.clone())
            });

        // Use Arc<Mutex<Vec>> for thread-safe sharing
        let streamed_outputs = Arc::new(Mutex::new(Vec::new()));
        let outputs_clone = streamed_outputs.clone();

        let result = installation
            .execute_check(&runner, move |chunk| {
                let mut outputs = outputs_clone.lock().unwrap();
                match chunk {
                    OutputChunk::Stdout(line) => outputs.push(format!("stdout: {}", line)),
                    OutputChunk::Stderr(line) => outputs.push(format!("stderr: {}", line)),
                }
            })
            .await;

        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(matches!(state, Installation::AlreadyInstalled { .. }));

        // Verify streamed outputs
        let outputs = streamed_outputs.lock().unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0], "stdout: Package found\n");
    }

    #[tokio::test]
    async fn test_execute_check_streaming_not_found() {
        let env_config = create_test_env_config();
        let installation = Installation::new(env_config).start();

        let mut runner = MockCommandRunner::new();

        // Add mock for execute_streaming
        let output = CommandOutput {
            stdout: String::new(),
            stderr: "Not found\n".to_string(),
            status: 1,
            success: false,
            duration: Duration::from_millis(100),
        };

        runner
            .expect_execute_streaming()
            .withf(|cmd, _, _| cmd == "test check")
            .returning(move |_, _, mut callback| {
                // Simulate streaming output
                callback(OutputChunk::Stderr("Not found\n".to_string()));
                Ok(output.clone())
            });

        // Use Arc<Mutex<Vec>> for thread-safe sharing
        let streamed_outputs = Arc::new(Mutex::new(Vec::new()));
        let outputs_clone = streamed_outputs.clone();

        let result = installation
            .execute_check(&runner, move |chunk| {
                let mut outputs = outputs_clone.lock().unwrap();
                match chunk {
                    OutputChunk::Stdout(line) => outputs.push(format!("stdout: {}", line)),
                    OutputChunk::Stderr(line) => outputs.push(format!("stderr: {}", line)),
                }
            })
            .await;

        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(matches!(state, Installation::NotAlreadyInstalled { .. }));

        // Verify streamed outputs
        let outputs = streamed_outputs.lock().unwrap();
        assert_eq!(outputs.len(), 1);
        assert_eq!(outputs[0], "stderr: Not found\n");
    }

    #[tokio::test]
    async fn test_execute_install_streaming_success() {
        let env_config = create_test_env_config();
        let installation = Installation::new(env_config)
            .start()
            .mark_not_already_installed();

        let mut runner = MockCommandRunner::new();

        // Add mock for execute_streaming
        let output = CommandOutput {
            stdout: "Installing...\nInstalled successfully\n".to_string(),
            stderr: String::new(),
            status: 0,
            success: true,
            duration: Duration::from_millis(200),
        };

        runner
            .expect_execute_streaming()
            .withf(|cmd, _, _| cmd == "test install")
            .returning(move |_, _, mut callback| {
                // Simulate streaming output with multiple lines
                callback(OutputChunk::Stdout("Installing...\n".to_string()));
                callback(OutputChunk::Stdout("Installed successfully\n".to_string()));
                Ok(output.clone())
            });

        // Use Arc<Mutex<Vec>> for thread-safe sharing
        let streamed_outputs = Arc::new(Mutex::new(Vec::new()));
        let outputs_clone = streamed_outputs.clone();

        let result = installation
            .execute_install(&runner, move |chunk| {
                let mut outputs = outputs_clone.lock().unwrap();
                match chunk {
                    OutputChunk::Stdout(line) => outputs.push(format!("stdout: {}", line)),
                    OutputChunk::Stderr(line) => outputs.push(format!("stderr: {}", line)),
                }
            })
            .await;

        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(matches!(state, Installation::Complete { .. }));

        // Verify streamed outputs
        let outputs = streamed_outputs.lock().unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0], "stdout: Installing...\n");
        assert_eq!(outputs[1], "stdout: Installed successfully\n");
    }

    #[tokio::test]
    async fn test_execute_install_streaming_failure() {
        let env_config = create_test_env_config();
        let installation = Installation::new(env_config)
            .start()
            .mark_not_already_installed();

        let mut runner = MockCommandRunner::new();

        // Add mock for execute_streaming
        let output = CommandOutput {
            stdout: "Starting installation...\n".to_string(),
            stderr: "Error: Installation failed\n".to_string(),
            status: 1,
            success: false,
            duration: Duration::from_millis(200),
        };

        runner
            .expect_execute_streaming()
            .withf(|cmd, _, _| cmd == "test install")
            .returning(move |_, _, mut callback| {
                // Simulate streaming output with stdout and stderr
                callback(OutputChunk::Stdout(
                    "Starting installation...\n".to_string(),
                ));
                callback(OutputChunk::Stderr(
                    "Error: Installation failed\n".to_string(),
                ));
                Ok(output.clone())
            });

        // Use Arc<Mutex<Vec>> for thread-safe sharing
        let streamed_outputs = Arc::new(Mutex::new(Vec::new()));
        let outputs_clone = streamed_outputs.clone();

        let result = installation
            .execute_install(&runner, move |chunk| {
                let mut outputs = outputs_clone.lock().unwrap();
                match chunk {
                    OutputChunk::Stdout(line) => outputs.push(format!("stdout: {}", line)),
                    OutputChunk::Stderr(line) => outputs.push(format!("stderr: {}", line)),
                }
            })
            .await;

        assert!(result.is_ok());
        let state = result.unwrap();
        assert!(matches!(state, Installation::Failed { .. }));

        // Verify streamed outputs
        let outputs = streamed_outputs.lock().unwrap();
        assert_eq!(outputs.len(), 2);
        assert_eq!(outputs[0], "stdout: Starting installation...\n");
        assert_eq!(outputs[1], "stderr: Error: Installation failed\n");
    }

    #[tokio::test]
    async fn test_execute_streaming_error_handling() {
        let env_config = create_test_env_config();
        let installation = Installation::new(env_config)
            .start()
            .mark_not_already_installed();

        let mut runner = MockCommandRunner::new();

        // Mock a timeout error
        runner
            .expect_execute_streaming()
            .withf(|cmd, _, _| cmd == "test install")
            .returning(|_, timeout, _| Err(CommandError::Timeout(timeout)));

        // Execute the command
        let result = installation.execute_install(&runner, |_| {}).await;

        assert!(result.is_ok());
        let state = result.unwrap();

        // Verify it transitioned to Failed state with appropriate message
        match state {
            Installation::Failed { error_message, .. } => {
                assert!(error_message.contains("Command timed out after 600s"));
            }
            _ => panic!("Expected Failed state, got {:?}", state),
        }
    }

    #[test]
    fn test_into_result() {
        let env_config = create_test_env_config();

        // Test AlreadyInstalled result
        let installation = Installation::new(env_config.clone())
            .start()
            .mark_already_installed();

        let result = installation
            .into_result("test-package".to_string())
            .unwrap();
        assert_eq!(result.package_name, "test-package");
        assert_eq!(result.status, InstallationStatus::AlreadyInstalled);

        // Test Complete result
        let output = CommandOutput {
            stdout: "success".to_string(),
            stderr: String::new(),
            status: 0,
            success: true,
            duration: Duration::from_millis(200),
        };

        let installation = Installation::new(env_config.clone())
            .start()
            .mark_not_already_installed()
            .start_installing()
            .complete(output);

        let result = installation
            .into_result("test-package".to_string())
            .unwrap();
        assert_eq!(result.status, InstallationStatus::Complete);
        assert!(result.command_output.is_some());

        // Test Failed result
        let installation = Installation::new(env_config)
            .start()
            .fail("Test error".to_string());

        let result = installation
            .into_result("test-package".to_string())
            .unwrap_err();

        assert!(matches!(result, InstallationError::InstallationFailed(_)));
    }

    #[test]
    fn test_installation_result_with_dependencies() {
        let result = InstallationReport {
            package_name: "main".to_string(),
            status: InstallationStatus::Complete,
            duration: Duration::from_secs(5),
            command_output: None,
            dependencies: Vec::new(),
        };

        let dep1 = InstallationReport {
            package_name: "dep1".to_string(),
            status: InstallationStatus::Complete,
            duration: Duration::from_secs(3),
            command_output: None,
            dependencies: Vec::new(),
        };

        let dep2 = InstallationReport {
            package_name: "dep2".to_string(),
            status: InstallationStatus::Complete,
            duration: Duration::from_secs(2),
            command_output: None,
            dependencies: Vec::new(),
        };

        let result_with_deps = result.with_dependencies(vec![dep1, dep2]);

        // Check that dependencies were added
        assert_eq!(result_with_deps.dependencies.len(), 2);
        assert_eq!(result_with_deps.dependencies[0].package_name, "dep1");
        assert_eq!(result_with_deps.dependencies[1].package_name, "dep2");

        // Check duration calculations
        assert_eq!(result_with_deps.duration, Duration::from_secs(5));
        assert_eq!(result_with_deps.total_duration(), Duration::from_secs(10)); // 5 + 3 + 2
        assert_eq!(
            result_with_deps.dependency_duration(),
            Duration::from_secs(5)
        ); // 3 + 2
    }
}
