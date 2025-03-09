// src/adapters/progress.rs
// Simplified progress reporting without indicatif dependency

use std::time::Duration;

use console::{style, Emoji};

use crate::domain::{config::AppConfig, installation::InstallationStatus};

// Define emojis with fallbacks
static INFO_EMOJI: Emoji<'_, '_> = Emoji("ℹ️ ", "[i] ");
static SUCCESS_EMOJI: Emoji<'_, '_> = Emoji("✅ ", "[√] ");
static ERROR_EMOJI: Emoji<'_, '_> = Emoji("❌ ", "[x] ");
static WARNING_EMOJI: Emoji<'_, '_> = Emoji("⚠️ ", "[!] ");

/// Types of status messages
#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    Info,
    Success,
    Error,
    Warning,
}

/// Streamlined progress manager
#[derive(Default)]
pub struct ProgressManager {
    use_colors: bool,
    verbose: bool,
}

impl ProgressManager {
    /// Create a new progress manager
    pub fn new(use_colors: bool, verbose: bool) -> Self {
        Self {
            use_colors,
            verbose,
        }
    }

    /// Format installation status update
    pub fn format_status_update(
        &self,
        status: &InstallationStatus,
        duration: Option<Duration>,
    ) -> String {
        match status {
            InstallationStatus::NotStarted => "Waiting to start...".to_string(),
            InstallationStatus::Checking => "Checking if already installed...".to_string(),
            InstallationStatus::NotInstalled => {
                let status_text = if self.use_colors {
                    style("Not installed").red().to_string()
                } else {
                    "Not installed".to_string()
                };
                self.with_duration(&status_text, duration)
            }
            InstallationStatus::AlreadyInstalled => {
                let status_text = if self.use_colors {
                    style("Already installed").green().to_string()
                } else {
                    "Already installed".to_string()
                };
                self.with_duration(&status_text, duration)
            }
            InstallationStatus::Installing => "Installing...".to_string(),
            InstallationStatus::Complete => {
                let status_text = if self.use_colors {
                    style("Installation complete").green().to_string()
                } else {
                    "Installation complete".to_string()
                };
                self.with_duration(&status_text, duration)
            }
            InstallationStatus::Failed(reason) => {
                let status_text = if self.use_colors {
                    format!("Installation failed: {}", style(reason).red().bold())
                } else {
                    format!("Installation failed: {}", reason)
                };
                self.with_duration(&status_text, duration)
            }
            InstallationStatus::Skipped(reason) => {
                let status_text = if self.use_colors {
                    format!("Installation skipped: {}", style(reason).yellow())
                } else {
                    format!("Installation skipped: {}", reason)
                };
                self.with_duration(&status_text, duration)
            }
        }
    }

    /// Format a status message
    pub fn status_line(&self, message_type: MessageType, message: &str) -> String {
        let prefix = match message_type {
            MessageType::Info => INFO_EMOJI,
            MessageType::Success => SUCCESS_EMOJI,
            MessageType::Error => ERROR_EMOJI,
            MessageType::Warning => WARNING_EMOJI,
        };

        let formatted_message = if self.use_colors {
            match message_type {
                MessageType::Info => style(message).blue().to_string(),
                MessageType::Success => style(message).green().to_string(),
                MessageType::Error => style(message).red().bold().to_string(),
                MessageType::Warning => style(message).yellow().bold().to_string(),
            }
        } else {
            message.to_string()
        };

        format!("{}{}", prefix, formatted_message)
    }

    /// Format a message with an error
    pub fn error(&self, message: &str) -> String {
        self.status_line(MessageType::Error, message)
    }

    /// Format a message with a success indicator
    pub fn success(&self, message: &str) -> String {
        self.status_line(MessageType::Success, message)
    }

    /// Format a message with an info indicator
    pub fn info(&self, message: &str) -> String {
        self.status_line(MessageType::Info, message)
    }

    /// Format a message with a warning indicator
    pub fn warning(&self, message: &str) -> String {
        self.status_line(MessageType::Warning, message)
    }

    /// Format a duration as human-readable
    pub fn format_duration(&self, duration: Duration) -> String {
        let total_seconds = duration.as_secs_f64();

        if total_seconds < 0.5 {
            format!("{:.1}ms", duration.as_millis() as f64)
        } else if total_seconds < 1.0 {
            format!("{:.2}s", total_seconds as f32)
        } else if total_seconds < 60.0 {
            format!("{:.1}s", total_seconds)
        } else {
            let minutes = (total_seconds / 60.0).floor();
            let seconds = total_seconds % 60.0;
            format!("{}m {:.1}s", minutes, seconds)
        }
    }

    /// Add a duration to a message
    fn with_duration(&self, message: &str, duration: Option<Duration>) -> String {
        if let Some(duration) = duration {
            format!("{} ({})", message, self.format_duration(duration))
        } else {
            message.to_string()
        }
    }

    /// Returns whether colors are enabled
    pub fn use_colors(&self) -> bool {
        self.use_colors
    }

    /// Returns whether verbose output is enabled
    pub fn verbose(&self) -> bool {
        self.verbose
    }

    /// Print a simple progress message (replacement for progress bars)
    pub fn print_progress(&self, message: &str) {
        println!("{}", message);
    }

    /// Print a success message
    pub fn print_success(&self, message: &str) {
        println!("{}", self.success(message));
    }

    /// Print an error message
    pub fn print_error(&self, message: &str) {
        println!("{}", self.error(message));
    }

    /// Print an info message
    pub fn print_info(&self, message: &str) {
        println!("{}", self.info(message));
    }

    /// Print a warning message
    pub fn print_warning(&self, message: &str) {
        println!("{}", self.warning(message));
    }

    /// Print verbose output if verbose mode is enabled
    pub fn print_verbose(&self, message: &str) {
        if self.verbose {
            println!("  {}", message);
        }
    }
}

impl<'a> From<&'a AppConfig> for ProgressManager {
    fn from(config: &'a AppConfig) -> Self {
        Self {
            use_colors: config.use_colors(),
            verbose: config.verbose(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::config::AppConfigBuilder;

    #[test]
    fn test_progress_manager_from_config() {
        let config = AppConfigBuilder::default()
            .environment("test-env")
            .package_directory("/test/path")
            .verbose(true)
            .use_colors(false)
            .use_unicode(true)
            .build();

        let manager = ProgressManager::from(&config);

        assert!(manager.verbose());
        assert!(!manager.use_colors());
    }

    #[test]
    fn test_format_status_update() {
        let manager = ProgressManager::default();

        // Test various status updates
        let checking = manager.format_status_update(&InstallationStatus::Checking, None);
        assert_eq!(checking, "Checking if already installed...");

        let not_installed = manager.format_status_update(
            &InstallationStatus::NotInstalled,
            Some(Duration::from_millis(200)),
        );
        assert_eq!(not_installed, "Not installed (200.0ms)");

        let already_installed = manager.format_status_update(
            &InstallationStatus::AlreadyInstalled,
            Some(Duration::from_secs(1)),
        );
        assert_eq!(already_installed, "Already installed (1.0s)");

        let failed = manager
            .format_status_update(&InstallationStatus::Failed("test error".to_string()), None);
        assert_eq!(failed, "Installation failed: test error");
    }

    #[test]
    fn test_status_line() {
        // Test without colors
        let manager = ProgressManager::default();

        let info = manager.status_line(MessageType::Info, "Info message");
        assert!(info.contains("ℹ️"));
        assert!(info.contains("Info message"));

        let success = manager.status_line(MessageType::Success, "Success message");
        assert!(success.contains("✅"));
        assert!(success.contains("Success message"));

        let error = manager.status_line(MessageType::Error, "Error message");
        assert!(error.contains("❌"));
        assert!(error.contains("Error message"));

        let warning = manager.status_line(MessageType::Warning, "Warning message");
        assert!(warning.contains("⚠️"));
        assert!(warning.contains("Warning message"));
    }

    #[test]
    fn test_format_duration() {
        let manager = ProgressManager::default();

        // Test different duration ranges
        assert_eq!(manager.format_duration(Duration::from_millis(50)), "50.0ms");
        assert_eq!(manager.format_duration(Duration::from_millis(500)), "0.50s");
        assert_eq!(manager.format_duration(Duration::from_secs(5)), "5.0s");
        assert_eq!(manager.format_duration(Duration::from_secs(90)), "1m 30.0s");
    }
}
