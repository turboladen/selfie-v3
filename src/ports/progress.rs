use std::fmt::Display;
use std::time::Duration;

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
