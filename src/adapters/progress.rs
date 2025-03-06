// src/adapters/progress.rs
// Consolidated Progress Manager that handles all progress reporting functionality

use std::{
    collections::HashMap,
    fmt::Display,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use console::style;
use indicatif::{MultiProgress, ProgressBar, ProgressStyle};

use crate::domain::{config::AppConfig, installation::InstallationStatus};

/// Standard emojis for different message types
static INFO_EMOJI: (&str, &str) = ("ℹ️ ", "[i] ");
static SUCCESS_EMOJI: (&str, &str) = ("✅ ", "[√] ");
static ERROR_EMOJI: (&str, &str) = ("❌ ", "[x] ");
static WARNING_EMOJI: (&str, &str) = ("⚠️ ", "[!] ");
static LOADING_EMOJI: (&str, &str) = ("⌛ ", "[*] ");
static BULLET_EMOJI: (&str, &str) = ("• ", "[*] ");

/// Represents different types of messages that can be displayed to the user
#[derive(Debug, Clone, PartialEq)]
pub enum MessageType {
    /// Informational message
    Info,
    /// Success message
    Success,
    /// Error message
    Error,
    /// Warning message
    Warning,
    /// Loading/in-progress message
    Loading,
    /// Generic status message
    Status,
}

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

/// Consolidated Progress Manager that handles all progress reporting
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

        // Enable spinner animation for spinner-type progress bars
        if style_type == ProgressStyleType::Spinner {
            self.enable_spinner(&progress_bar);
        }

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

    /// Update a progress bar based on installation status
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
                // For a package that is already installed or was just installed,
                // just update the message, don't call finish_with_message
                if let Some(pb) = self.get_progress_bar(id) {
                    pb.set_message(message);
                    // Signal completion without duplicating the message
                    pb.finish();
                    Ok(())
                } else {
                    Err(format!("Progress bar with ID '{}' not found", id))
                }
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

    // Add this method to enable spinner ticking
    fn enable_spinner(&self, pb: &ProgressBar) {
        // Set a reasonable tick rate for spinners (100ms is a good default)
        pb.enable_steady_tick(Duration::from_millis(100));
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

    //
    // Methods moved from ProgressReporter and ConsoleRenderer
    //

    /// Create and render an info message
    pub fn info(&self, message: &str) -> String {
        self.format_message(MessageType::Info, message, None, None, 0)
    }

    /// Create and render a success message
    pub fn success(&self, message: &str) -> String {
        self.format_message(MessageType::Success, message, None, None, 0)
    }

    /// Create and render an error message
    pub fn error(&self, message: &str) -> String {
        self.format_message(MessageType::Error, message, None, None, 0)
    }

    /// Create and render a warning message
    pub fn warning(&self, message: &str) -> String {
        self.format_message(MessageType::Warning, message, None, None, 0)
    }

    /// Create and render a loading/in-progress message
    pub fn loading(&self, message: &str) -> String {
        self.format_message(MessageType::Loading, message, None, None, 0)
    }

    /// Create and render a status message with optional context and duration
    pub fn status(
        &self,
        message: &str,
        context: Option<&str>,
        duration: Option<Duration>,
        indent_level: usize,
    ) -> String {
        self.format_message(
            MessageType::Status,
            message,
            context.map(|s| s.to_string()),
            duration,
            indent_level,
        )
    }

    /// Render an error object directly
    pub fn render_error(&self, error: &dyn Display) -> String {
        self.format_message(
            MessageType::Error,
            &format!("Error: {}", error),
            None,
            None,
            0,
        )
    }

    /// Format a duration as a human-readable string
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

    /// Creates a formatted message with emoji/color based on message type
    fn format_message(
        &self,
        message_type: MessageType,
        text: &str,
        context: Option<String>,
        duration: Option<Duration>,
        indent_level: usize,
    ) -> String {
        // Create indentation based on level
        let indent = "  ".repeat(indent_level);

        // Get appropriate emoji based on type
        let emoji = self.get_emoji(&message_type);

        // Apply styling based on type
        let styled_text = self.style_text(text, &message_type);

        let mut result = format!("{}{}{}", indent, emoji, styled_text);

        if let Some(ctx) = context {
            result.push_str(&format!(": {}", ctx));
        }

        if let Some(dur) = duration {
            result.push_str(&format!(" ({})", self.format_duration(dur)));
        }

        result
    }

    /// Get the appropriate emoji (or text fallback) for a message type
    fn get_emoji(&self, message_type: &MessageType) -> &str {
        if self.use_unicode {
            match message_type {
                MessageType::Info => INFO_EMOJI.0,
                MessageType::Success => SUCCESS_EMOJI.0,
                MessageType::Error => ERROR_EMOJI.0,
                MessageType::Warning => WARNING_EMOJI.0,
                MessageType::Loading => LOADING_EMOJI.0,
                MessageType::Status => BULLET_EMOJI.0,
            }
        } else {
            match message_type {
                MessageType::Info => INFO_EMOJI.1,
                MessageType::Success => SUCCESS_EMOJI.1,
                MessageType::Error => ERROR_EMOJI.1,
                MessageType::Warning => WARNING_EMOJI.1,
                MessageType::Loading => LOADING_EMOJI.1,
                MessageType::Status => BULLET_EMOJI.1,
            }
        }
    }

    /// Apply appropriate styling to text based on message type
    fn style_text(&self, text: &str, message_type: &MessageType) -> String {
        if !self.use_colors {
            return text.to_string();
        }

        match message_type {
            MessageType::Info => style(text).blue().to_string(),
            MessageType::Success => style(text).green().to_string(),
            MessageType::Error => style(text).red().bold().to_string(),
            MessageType::Warning => style(text).yellow().bold().to_string(),
            MessageType::Loading => style(text).cyan().to_string(),
            MessageType::Status => text.to_string(),
        }
    }

    /// Create a status line with specific styling
    pub fn status_line(&self, message_type: MessageType, message: &str) -> String {
        match message_type {
            MessageType::Info => self.info(message),
            MessageType::Success => self.success(message),
            MessageType::Error => self.error(message),
            MessageType::Warning => self.warning(message),
            MessageType::Loading => self.loading(message),
            MessageType::Status => self.status(message, None, None, 0),
        }
    }

    /// Returns whether colors are enabled for this progress manager
    pub fn use_colors(&self) -> bool {
        self.use_colors
    }

    pub fn verbose(&self) -> bool {
        self.verbose
    }
}

impl<'a> From<&'a AppConfig> for ProgressManager {
    /// Create a new progress manager from AppConfig
    fn from(config: &'a AppConfig) -> Self {
        Self {
            multi_progress: Arc::new(MultiProgress::new()),
            progress_bars: Arc::new(Mutex::new(HashMap::new())),
            use_colors: config.use_colors(),
            use_unicode: config.use_unicode(),
            verbose: config.verbose(),
        }
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
    use crate::domain::{config::AppConfigBuilder, installation::InstallationStatus};
    use std::thread;

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
