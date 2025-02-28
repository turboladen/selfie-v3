// src/progress_display.rs
// Implements interactive progress display using indicatif with support for
// multiple progress bars, spinners, and multi-line output.

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use console::{style, Color};
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::{installation::InstallationStatus, progress::MessageType};

/// Represents the style of a progress element
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum ProgressStyleType {
    /// Progress bar with a spinner
    Spinner,
    /// Progress bar with progress percentage
    Bar,
    /// Simple message display (no bar)
    Message,
}

/// Responsible for managing and rendering interactive progress displays
pub struct ProgressManager {
    /// Multi-progress instance for managing multiple progress bars
    multi_progress: Arc<MultiProgress>,
    /// Map of progress bars by their IDs
    progress_bars: Arc<Mutex<HashMap<String, ProgressBar>>>,
    /// Whether to use colors
    use_colors: bool,
    /// Whether terminal supports Unicode
    use_unicode: bool,
    /// Whether to enable verbose output
    verbose: bool,
}

impl ProgressManager {
    /// Create a new progress manager with the specified options
    pub fn new(use_colors: bool, use_unicode: bool, verbose: bool) -> Self {
        Self {
            multi_progress: Arc::new(MultiProgress::new()),
            progress_bars: Arc::new(Mutex::new(HashMap::new())),
            use_colors,
            use_unicode,
            verbose,
        }
    }

    /// Create a progress bar with the specified style
    pub fn create_progress_bar(
        &self,
        id: &str,
        message: &str,
        style_type: ProgressStyleType,
    ) -> ProgressBar {
        let pb = match style_type {
            ProgressStyleType::Spinner => self.create_spinner_style(),
            ProgressStyleType::Bar => self.create_bar_style(),
            ProgressStyleType::Message => self.create_message_style(),
        };

        let progress_bar = self.multi_progress.add(pb);
        progress_bar.set_message(message.to_string());

        // Store the progress bar for later reference
        self.progress_bars
            .lock()
            .unwrap()
            .insert(id.to_string(), progress_bar.clone());

        progress_bar
    }

    /// Get an existing progress bar by ID
    pub fn get_progress_bar(&self, id: &str) -> Option<ProgressBar> {
        self.progress_bars.lock().unwrap().get(id).cloned()
    }

    /// Update a progress bar with a new message
    pub fn update_progress(&self, id: &str, message: &str) -> Result<(), String> {
        if let Some(pb) = self.get_progress_bar(id) {
            pb.set_message(message.to_string());
            Ok(())
        } else {
            Err(format!("Progress bar with ID '{}' not found", id))
        }
    }

    /// Mark a progress operation as completed with custom message
    pub fn complete_progress(&self, id: &str, message: &str) -> Result<(), String> {
        if let Some(pb) = self.get_progress_bar(id) {
            pb.finish_with_message(message.to_string());
            Ok(())
        } else {
            Err(format!("Progress bar with ID '{}' not found", id))
        }
    }

    /// Update a progress bar based on installation status with color support
    pub fn update_from_status(
        &self,
        id: &str,
        status: &InstallationStatus,
        duration: Option<Duration>,
    ) -> Result<(), String> {
        let message = match status {
            InstallationStatus::NotStarted => "Waiting to start...".to_string(),
            InstallationStatus::Checking => "Checking if already installed...".to_string(),
            InstallationStatus::NotInstalled => {
                let status_text = if self.use_colors {
                    style("Not installed").red().to_string()
                } else {
                    "Not installed".to_string()
                };
                self.format_status_with_duration(&status_text, duration)
            }
            InstallationStatus::AlreadyInstalled => {
                let status_text = if self.use_colors {
                    style("Already installed").green().to_string()
                } else {
                    "Already installed".to_string()
                };
                self.format_status_with_duration(&status_text, duration)
            }
            InstallationStatus::Installing => "Installing...".to_string(),
            InstallationStatus::Complete => {
                let status_text = if self.use_colors {
                    style("Installation complete").green().to_string()
                } else {
                    "Installation complete".to_string()
                };
                self.format_status_with_duration(&status_text, duration)
            }
            InstallationStatus::Failed(reason) => {
                let status_text = if self.use_colors {
                    format!("Installation failed: {}", style(reason).red().bold())
                } else {
                    format!("Installation failed: {}", reason)
                };
                self.format_status_with_duration(&status_text, duration)
            }
            InstallationStatus::Skipped(reason) => {
                let status_text = if self.use_colors {
                    format!("Installation skipped: {}", style(reason).yellow())
                } else {
                    format!("Installation skipped: {}", reason)
                };
                self.format_status_with_duration(&status_text, duration)
            }
        };

        match status {
            InstallationStatus::Complete | InstallationStatus::AlreadyInstalled => {
                self.complete_progress(id, &message)
            }
            InstallationStatus::Failed(_) | InstallationStatus::Skipped(_) => {
                if let Some(pb) = self.get_progress_bar(id) {
                    pb.abandon_with_message(message);
                    Ok(())
                } else {
                    Err(format!("Progress bar with ID '{}' not found", id))
                }
            }
            _ => self.update_progress(id, &message),
        }
    }

    /// Create a new progress manager for a nested task
    pub fn create_task_progress_manager(&self, message: &str) -> ProgressDisplay {
        let pb = self.create_progress_bar(
            &format!("task-{}", rand::random::<u64>()),
            message,
            ProgressStyleType::Spinner,
        );
        ProgressDisplay::new(pb, self.verbose)
    }

    /// Format a status message with an optional duration
    fn format_status_with_duration(&self, message: &str, duration: Option<Duration>) -> String {
        if let Some(duration) = duration {
            format!("{} ({:.1?})", message, duration)
        } else {
            message.to_string()
        }
    }

    /// Create a spinner style progress bar
    fn create_spinner_style(&self) -> ProgressBar {
        let pb = ProgressBar::new_spinner();

        let spinner = if self.use_unicode {
            "⠋⠙⠹⠸⠼⠴⠦⠧⠇⠏"
        } else {
            "-\\|/"
        };

        let template = if self.use_colors {
            format!(
                "{{spinner:.{}}} {{msg}}",
                if self.use_colors { "cyan" } else { "" }
            )
        } else {
            "{spinner} {msg}".to_string()
        };

        pb.set_style(
            ProgressStyle::default_spinner()
                .tick_chars(spinner)
                .template(&template)
                .unwrap(),
        );

        pb
    }

    /// Create a bar style progress bar
    fn create_bar_style(&self) -> ProgressBar {
        let pb = ProgressBar::new(100);

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

        pb.set_style(
            ProgressStyle::default_bar()
                .progress_chars(chars)
                .template(template)
                .unwrap(),
        );

        pb
    }

    /// Create a message style progress bar (no actual bar)
    fn create_message_style(&self) -> ProgressBar {
        let pb = ProgressBar::new(1);
        pb.set_style(ProgressStyle::default_spinner().template("{msg}").unwrap());
        pb
    }

    /// Add a line of text under a progress bar
    pub fn add_progress_line(&self, id: &str, line: &str) -> Result<(), String> {
        if let Some(pb) = self.get_progress_bar(id) {
            if self.verbose {
                pb.println(line);
            }
            Ok(())
        } else {
            Err(format!("Progress bar with ID '{}' not found", id))
        }
    }

    /// Add command output under a progress bar
    pub fn add_command_output(
        &self,
        id: &str,
        output_type: &str,
        content: &str,
    ) -> Result<(), String> {
        if self.verbose {
            let line = format!("  {}: {}", output_type, content.trim());
            self.add_progress_line(id, &line)
        } else {
            Ok(())
        }
    }

    /// Create a status message line with appropriate styling
    pub fn status_line(&self, message_type: MessageType, message: &str) -> String {
        let (emoji, color) = match message_type {
            MessageType::Info => ("ℹ️ ", Color::Blue),
            MessageType::Success => ("✓ ", Color::Green),
            MessageType::Error => ("✗ ", Color::Red),
            MessageType::Warning => ("⚠️ ", Color::Yellow),
            MessageType::Loading => ("⌛ ", Color::Cyan),
            MessageType::Status => ("• ", Color::White),
        };

        if self.use_colors {
            format!("{}{}", emoji, style(message).fg(color))
        } else {
            let text_emoji = match message_type {
                MessageType::Info => "[i] ",
                MessageType::Success => "[√] ",
                MessageType::Error => "[x] ",
                MessageType::Warning => "[!] ",
                MessageType::Loading => "[*] ",
                MessageType::Status => "[•] ",
            };
            format!("{}{}", text_emoji, message)
        }
    }

    /// Returns whether colors are enabled for this progress manager
    pub fn use_colors(&self) -> bool {
        self.use_colors
    }
}

/// Represents a progress display for a single task
pub struct ProgressDisplay {
    /// The progress bar for this task
    progress_bar: ProgressBar,
    /// Start time of the task
    start_time: Instant,
    /// Whether verbose output is enabled
    verbose: bool,
}

impl ProgressDisplay {
    /// Create a new progress display for a task
    pub fn new(progress_bar: ProgressBar, verbose: bool) -> Self {
        Self {
            progress_bar,
            start_time: Instant::now(),
            verbose,
        }
    }

    /// Update the progress message
    pub fn update(&self, message: &str) {
        self.progress_bar.set_message(message.to_string());
    }

    /// Complete the progress with a success message
    pub fn success(&self, message: &str) {
        let duration = self.start_time.elapsed();
        let message_with_time = format!("{} ({:.1?})", message, duration);
        self.progress_bar.finish_with_message(message_with_time);
    }

    /// Complete the progress with an error message
    pub fn error(&self, message: &str) {
        let duration = self.start_time.elapsed();
        let message_with_time = format!("{} ({:.1?})", message, duration);
        self.progress_bar.abandon_with_message(message_with_time);
    }

    /// Add a line of output text
    pub fn add_line(&self, text: &str) {
        if self.verbose {
            self.progress_bar.println(text);
        }
    }

    /// Add command output
    pub fn add_output(&self, output_type: &str, content: &str) {
        if self.verbose {
            let line = format!("  {}: {}", output_type, content.trim());
            self.progress_bar.println(line);
        }
    }

    /// Get the elapsed time since the start
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::installation::InstallationStatus;
    use crate::progress::MessageType;
    use std::thread;

    #[test]
    fn test_create_progress_bar() {
        let manager = ProgressManager::new(false, false, true);
        let _pb = manager.create_progress_bar("test", "Test Progress", ProgressStyleType::Spinner);
        assert!(manager.get_progress_bar("test").is_some());
    }

    #[test]
    fn test_update_progress() {
        let manager = ProgressManager::new(false, false, true);
        manager.create_progress_bar("test", "Test Progress", ProgressStyleType::Spinner);

        assert!(manager.update_progress("test", "Updated Message").is_ok());
        assert!(manager.update_progress("nonexistent", "Message").is_err());
    }

    #[test]
    fn test_complete_progress() {
        let manager = ProgressManager::new(false, false, true);
        manager.create_progress_bar("test", "Test Progress", ProgressStyleType::Spinner);

        assert!(manager.complete_progress("test", "Completed").is_ok());
        assert!(manager
            .complete_progress("nonexistent", "Completed")
            .is_err());
    }

    #[test]
    fn test_update_from_status() {
        let manager = ProgressManager::new(false, false, true);
        manager.create_progress_bar("test", "Test Progress", ProgressStyleType::Spinner);

        assert!(manager
            .update_from_status("test", &InstallationStatus::Checking, None)
            .is_ok());
        assert!(manager
            .update_from_status(
                "test",
                &InstallationStatus::Complete,
                Some(Duration::from_secs(5))
            )
            .is_ok());
        assert!(manager
            .update_from_status("nonexistent", &InstallationStatus::Complete, None)
            .is_err());
    }

    #[test]
    fn test_format_status_with_duration() {
        let manager = ProgressManager::new(false, false, true);

        let without_duration = manager.format_status_with_duration("Test Message", None);
        assert_eq!(without_duration, "Test Message");

        let with_duration =
            manager.format_status_with_duration("Test Message", Some(Duration::from_secs(5)));
        assert!(with_duration.contains("Test Message"));
        assert!(with_duration.contains("5.0s"));
    }

    #[test]
    fn test_progress_display() {
        let manager = ProgressManager::new(false, false, true);
        let pb = manager.create_progress_bar("test", "Test Progress", ProgressStyleType::Spinner);
        let display = ProgressDisplay::new(pb, true);

        display.update("Working...");
        thread::sleep(Duration::from_millis(10)); // Ensure elapsed time is non-zero

        let elapsed = display.elapsed();
        assert!(elapsed.as_millis() > 0);

        display.success("Done!");
    }

    #[test]
    fn test_status_line_formatting() {
        // Test without colors
        let manager = ProgressManager::new(false, false, true);

        let info_line = manager.status_line(MessageType::Info, "Info message");
        assert!(info_line.contains("[i]"));
        assert!(info_line.contains("Info message"));

        let success_line = manager.status_line(MessageType::Success, "Success message");
        assert!(success_line.contains("[√]"));
        assert!(success_line.contains("Success message"));

        let error_line = manager.status_line(MessageType::Error, "Error message");
        assert!(error_line.contains("[x]"));
        assert!(error_line.contains("Error message"));
    }
}
