// src/progress.rs
// Implements core progress reporting functionality for displaying status messages,
// errors, warnings, and other feedback to the user.

use std::fmt::Display;
use std::time::Duration;

use console::{style, Emoji};

// Define emoji constants for different message types
static INFO_EMOJI: Emoji = Emoji("ℹ️ ", "[i] ");
static SUCCESS_EMOJI: Emoji = Emoji("✅ ", "[√] ");
static ERROR_EMOJI: Emoji = Emoji("❌ ", "[x] ");
static WARNING_EMOJI: Emoji = Emoji("⚠️ ", "[!] ");
static LOADING_EMOJI: Emoji = Emoji("⌛ ", "[*] ");
static BULLET_EMOJI: Emoji = Emoji("• ", "[*] ");

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

/// Represents a message to be displayed to the user
#[derive(Debug, Clone, PartialEq)]
pub struct Message {
    /// The type of message
    pub message_type: MessageType,
    /// The main message text
    pub text: String,
    /// Optional context or details for the message
    pub context: Option<String>,
    /// Optional duration information (e.g., for timing operations)
    pub duration: Option<Duration>,
    /// Optional indentation level for nested messages
    pub indent_level: usize,
}

/// Defines how messages should be rendered for display
pub trait MessageRenderer: Send + Sync {
    /// Render a message as a string
    fn render(&self, message: &Message) -> String;

    /// Render an error as a string
    fn render_error(&self, error: &dyn Display) -> String;

    /// Format a duration as a human-readable string
    fn render_duration(&self, duration: Duration) -> String;
}

/// Renderer that formats messages for console output with optional color and emoji support
#[derive(Clone)]
pub struct ConsoleRenderer {
    /// Whether to use emoji in output
    pub use_emoji: bool,
    /// Whether to use colors in output
    pub use_colors: bool,
}

impl ConsoleRenderer {
    /// Create a new ConsoleRenderer with the specified options
    pub fn new(use_emoji: bool, use_colors: bool) -> Self {
        Self {
            use_emoji,
            use_colors,
        }
    }

    /// Get the appropriate emoji (or text fallback) for a message type
    fn get_emoji(&self, message_type: &MessageType) -> &str {
        if self.use_emoji {
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
}

impl MessageRenderer for ConsoleRenderer {
    fn render(&self, message: &Message) -> String {
        // Create indentation based on level
        let indent = "  ".repeat(message.indent_level);

        let emoji = self.get_emoji(&message.message_type);
        let text = self.style_text(&message.text, &message.message_type);

        let mut result = format!("{}{}{}", indent, emoji, text);

        if let Some(context) = &message.context {
            result.push_str(&format!(": {}", context));
        }

        if let Some(duration) = message.duration {
            result.push_str(&format!(" ({})", self.render_duration(duration)));
        }

        result
    }

    fn render_error(&self, error: &dyn Display) -> String {
        let error_message = Message {
            message_type: MessageType::Error,
            text: format!("Error: {}", error),
            context: None,
            duration: None,
            indent_level: 0,
        };

        self.render(&error_message)
    }

    fn render_duration(&self, duration: Duration) -> String {
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
}

/// Helper for creating and rendering different types of messages
pub struct ProgressReporter {
    renderer: Box<dyn MessageRenderer>,
}

impl Clone for ProgressReporter {
    fn clone(&self) -> Self {
        // We can't clone the MessageRenderer directly,
        // so we need to create a new one
        // In this case, we'll use a ConsoleRenderer as a default fallback
        let console_renderer = Box::new(ConsoleRenderer::new(true, true));
        Self {
            renderer: console_renderer,
        }
    }
}

impl ProgressReporter {
    /// Create a new ProgressReporter with the specified renderer
    pub fn new(renderer: Box<dyn MessageRenderer>) -> Self {
        Self { renderer }
    }

    /// Create and render an info message
    pub fn info(&self, message: &str) -> String {
        let msg = Message {
            message_type: MessageType::Info,
            text: message.to_string(),
            context: None,
            duration: None,
            indent_level: 0,
        };

        self.renderer.render(&msg)
    }

    /// Create and render a success message
    pub fn success(&self, message: &str) -> String {
        let msg = Message {
            message_type: MessageType::Success,
            text: message.to_string(),
            context: None,
            duration: None,
            indent_level: 0,
        };

        self.renderer.render(&msg)
    }

    /// Create and render an error message
    pub fn error(&self, message: &str) -> String {
        let msg = Message {
            message_type: MessageType::Error,
            text: message.to_string(),
            context: None,
            duration: None,
            indent_level: 0,
        };

        self.renderer.render(&msg)
    }

    /// Create and render a warning message
    pub fn warning(&self, message: &str) -> String {
        let msg = Message {
            message_type: MessageType::Warning,
            text: message.to_string(),
            context: None,
            duration: None,
            indent_level: 0,
        };

        self.renderer.render(&msg)
    }

    /// Create and render a loading/in-progress message
    pub fn loading(&self, message: &str) -> String {
        let msg = Message {
            message_type: MessageType::Loading,
            text: message.to_string(),
            context: None,
            duration: None,
            indent_level: 0,
        };

        self.renderer.render(&msg)
    }

    /// Create and render a status message with optional context and duration
    pub fn status(
        &self,
        message: &str,
        context: Option<&str>,
        duration: Option<Duration>,
        indent_level: usize,
    ) -> String {
        let msg = Message {
            message_type: MessageType::Status,
            text: message.to_string(),
            context: context.map(|s| s.to_string()),
            duration,
            indent_level,
        };

        self.renderer.render(&msg)
    }

    /// Render an error object directly
    pub fn render_error(&self, error: &dyn Display) -> String {
        self.renderer.render_error(error)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_console_renderer_without_emoji_colors() {
        let renderer = ConsoleRenderer::new(false, false);

        let message = Message {
            message_type: MessageType::Success,
            text: "Test message".to_string(),
            context: None,
            duration: None,
            indent_level: 0,
        };

        let rendered = renderer.render(&message);
        assert_eq!(rendered, "[√] Test message");
    }

    #[test]
    fn test_console_renderer_with_context() {
        let renderer = ConsoleRenderer::new(false, false);

        let message = Message {
            message_type: MessageType::Info,
            text: "Package".to_string(),
            context: Some("Details about the package".to_string()),
            duration: None,
            indent_level: 0,
        };

        let rendered = renderer.render(&message);
        assert_eq!(rendered, "[i] Package: Details about the package");
    }

    #[test]
    fn test_console_renderer_with_duration() {
        let renderer = ConsoleRenderer::new(false, false);

        let message = Message {
            message_type: MessageType::Success,
            text: "Installation complete".to_string(),
            context: None,
            duration: Some(Duration::from_secs(45)),
            indent_level: 0,
        };

        let rendered = renderer.render(&message);
        assert_eq!(rendered, "[√] Installation complete (45.0s)");
    }

    #[test]
    fn test_console_renderer_with_indentation() {
        let renderer = ConsoleRenderer::new(false, false);

        let message = Message {
            message_type: MessageType::Status,
            text: "Nested message".to_string(),
            context: None,
            duration: None,
            indent_level: 2,
        };

        let rendered = renderer.render(&message);
        assert_eq!(rendered, "    [*] Nested message");
    }

    #[test]
    fn test_console_renderer_with_everything() {
        let renderer = ConsoleRenderer::new(false, false);

        let message = Message {
            message_type: MessageType::Error,
            text: "Installation failed".to_string(),
            context: Some("Package not found".to_string()),
            duration: Some(Duration::from_millis(200)),
            indent_level: 1,
        };

        let rendered = renderer.render(&message);
        assert_eq!(
            rendered,
            "  [x] Installation failed: Package not found (0.20s)"
        );
    }

    #[test]
    fn test_duration_formatting() {
        let renderer = ConsoleRenderer::new(false, false);

        assert_eq!(renderer.render_duration(Duration::from_millis(50)), "50ms");
        assert_eq!(
            renderer.render_duration(Duration::from_millis(500)),
            "0.50s"
        );
        assert_eq!(renderer.render_duration(Duration::from_secs(2)), "2.0s");
        assert_eq!(renderer.render_duration(Duration::from_secs(45)), "45.0s");
        assert_eq!(
            renderer.render_duration(Duration::from_secs(90)),
            "1m 30.0s"
        );
        assert_eq!(
            renderer.render_duration(Duration::from_secs(3600)),
            "60m 0.0s"
        );
    }

    #[test]
    fn test_progress_reporter() {
        let renderer = Box::new(ConsoleRenderer::new(false, false));
        let reporter = ProgressReporter::new(renderer);

        assert_eq!(reporter.info("Information"), "[i] Information");
        assert_eq!(reporter.success("Success message"), "[√] Success message");
        assert_eq!(reporter.error("Error occurred"), "[x] Error occurred");
        assert_eq!(reporter.warning("Warning message"), "[!] Warning message");
        assert_eq!(reporter.loading("Loading..."), "[*] Loading...");

        let status = reporter.status(
            "Installation status",
            Some("All good"),
            Some(Duration::from_secs(5)),
            0,
        );
        assert_eq!(status, "[*] Installation status: All good (5.0s)");

        let nested_status =
            reporter.status("Dependency installation", Some("In progress"), None, 2);
        assert_eq!(
            nested_status,
            "    [*] Dependency installation: In progress"
        );
    }

    #[test]
    fn test_render_error() {
        let renderer = Box::new(ConsoleRenderer::new(false, false));
        let reporter = ProgressReporter::new(renderer);

        let error = std::io::Error::new(std::io::ErrorKind::NotFound, "File not found");
        let error_ref: &dyn Display = &error;
        assert_eq!(
            reporter.render_error(error_ref),
            "[x] Error: File not found"
        );
    }
}
