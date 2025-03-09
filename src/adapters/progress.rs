// src/adapters/progress.rs
// Streamlined progress reporting with direct use of indicatif

use std::time::Duration;

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::domain::{config::AppConfig, installation::InstallationStatus};

/// Style types for progress elements
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressStyleType {
    Spinner,
    Bar,
    Message,
}

/// Types of status messages
#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    Info,
    Success,
    Error,
    Warning,
}

/// Streamlined progress manager
pub struct ProgressManager {
    multi_progress: MultiProgress,
    use_colors: bool,
    use_unicode: bool,
    verbose: bool,
}

impl ProgressManager {
    /// Create a new progress manager
    pub fn new(use_colors: bool, use_unicode: bool, verbose: bool) -> Self {
        Self {
            multi_progress: MultiProgress::new(),
            use_colors,
            use_unicode,
            verbose,
        }
    }

    /// Create a progress bar
    pub fn create_progress_bar(&self, message: &str, style_type: ProgressStyleType) -> ProgressBar {
        let pb = match style_type {
            ProgressStyleType::Spinner => {
                let spinner = if self.use_unicode {
                    "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
                } else {
                    "-\\|/"
                };

                let template = if self.use_colors {
                    "{spinner:.cyan} {msg}"
                } else {
                    "{spinner} {msg}"
                };

                let pb = ProgressBar::new_spinner();
                pb.set_style(
                    ProgressStyle::default_spinner()
                        .tick_chars(spinner)
                        .template(template)
                        .unwrap(),
                );
                pb
            }
            ProgressStyleType::Bar => {
                let chars = if self.use_unicode {
                    "█▉▊▋▌▍▎▏ "
                } else {
                    "##--"
                };

                let template = if self.use_colors {
                    "  {bar:40.cyan/blue} {pos:>3}/{len:3} {msg}"
                } else {
                    "  {bar:40} {pos:>3}/{len:3} {msg}"
                };

                let pb = ProgressBar::new(100);
                pb.set_style(
                    ProgressStyle::default_bar()
                        .progress_chars(chars)
                        .template(template)
                        .unwrap(),
                );
                pb
            }
            ProgressStyleType::Message => {
                let pb = ProgressBar::new(1);
                pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
                pb
            }
        };

        let progress_bar = self.multi_progress.add(pb);
        progress_bar.set_message(message.to_string());

        // Enable spinner animation for spinner-type progress bars
        if style_type == ProgressStyleType::Spinner {
            progress_bar.enable_steady_tick(Duration::from_millis(100));
        }

        progress_bar
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
            MessageType::Info => {
                if self.use_unicode {
                    "ℹ️ "
                } else {
                    "[i] "
                }
            }
            MessageType::Success => {
                if self.use_unicode {
                    "✅ "
                } else {
                    "[√] "
                }
            }
            MessageType::Error => {
                if self.use_unicode {
                    "❌ "
                } else {
                    "[x] "
                }
            }
            MessageType::Warning => {
                if self.use_unicode {
                    "⚠️ "
                } else {
                    "[!] "
                }
            }
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

        if total_seconds < 0.1 {
            format!("{:.1}ms", duration.as_millis())
        } else if total_seconds < 1.0 {
            format!("{:.2}s", total_seconds)
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

    /// Returns whether unicode is enabled
    pub fn use_unicode(&self) -> bool {
        self.use_unicode
    }
}

impl<'a> From<&'a AppConfig> for ProgressManager {
    fn from(config: &'a AppConfig) -> Self {
        Self {
            multi_progress: MultiProgress::new(),
            use_colors: config.use_colors(),
            use_unicode: config.use_unicode(),
            verbose: config.verbose(),
        }
    }
}

/// Progress display for a single task
pub struct ProgressDisplay {
    progress_bar: ProgressBar,
    verbose: bool,
}

impl ProgressDisplay {
    /// Create a new progress display
    pub fn new(progress_bar: ProgressBar, verbose: bool) -> Self {
        Self {
            progress_bar,
            verbose,
        }
    }

    /// Update the progress message
    pub fn update(&self, message: &str) {
        self.progress_bar.set_message(message.to_string());
    }

    /// Complete the progress with success
    pub fn success(&self, message: &str) {
        self.progress_bar.finish_with_message(message.to_string());
    }

    /// Complete the progress with error
    pub fn error(&self, message: &str) {
        self.progress_bar.abandon_with_message(message.to_string());
    }

    /// Add output text when in verbose mode
    pub fn add_line(&self, text: &str) {
        if self.verbose {
            self.progress_bar.println(text);
        }
    }

    /// Add command output when in verbose mode
    pub fn add_output(&self, output_type: &str, content: &str) {
        if self.verbose {
            let line = format!("  {}: {}", output_type, content.trim());
            self.progress_bar.println(line);
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
        assert!(manager.use_unicode());
    }

    #[test]
    fn test_format_status_update() {
        let manager = ProgressManager::new(false, false, true);

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
        let manager = ProgressManager::new(false, false, true);

        let info = manager.status_line(MessageType::Info, "Info message");
        assert!(info.contains("[i]"));
        assert!(info.contains("Info message"));

        let success = manager.status_line(MessageType::Success, "Success message");
        assert!(success.contains("[√]"));
        assert!(success.contains("Success message"));

        let error = manager.status_line(MessageType::Error, "Error message");
        assert!(error.contains("[x]"));
        assert!(error.contains("Error message"));

        let warning = manager.status_line(MessageType::Warning, "Warning message");
        assert!(warning.contains("[!]"));
        assert!(warning.contains("Warning message"));
    }

    #[test]
    fn test_format_duration() {
        let manager = ProgressManager::new(false, false, true);

        // Test different duration ranges
        assert_eq!(manager.format_duration(Duration::from_millis(50)), "50.0ms");
        assert_eq!(manager.format_duration(Duration::from_millis(500)), "0.50s");
        assert_eq!(manager.format_duration(Duration::from_secs(5)), "5.0s");
        assert_eq!(manager.format_duration(Duration::from_secs(90)), "1m 30.0s");
    }

    #[test]
    fn test_progress_display() {
        let manager = ProgressManager::new(false, false, true);
        let pb = manager.create_progress_bar("Test progress", ProgressStyleType::Spinner);
        let display = ProgressDisplay::new(pb, true);

        // Test basic operations
        display.update("Working...");
        display.add_line("Output line");
        display.add_output("stdout", "Test output");
        display.success("Completed");
    }
}
