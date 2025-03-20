// src/adapters/command/output_buffer.rs

use crate::adapters::progress::ProgressManager;
use crate::ports::command::OutputChunk;
use std::fmt::Write;

/// Buffer that both displays and captures command output in real-time
#[derive(Clone)]
pub struct CommandOutputBuffer {
    /// Accumulated stdout content
    stdout_buffer: String,

    /// Accumulated stderr content
    stderr_buffer: String,

    /// Progress manager for displaying output
    progress_manager: ProgressManager,

    /// Indentation level for displayed output
    indent: String,

    /// Whether to display output (verbose mode)
    display_output: bool,
}

impl CommandOutputBuffer {
    /// Create a new command output buffer
    pub fn new(
        progress_manager: ProgressManager,
        indent_level: usize,
        display_output: bool,
    ) -> Self {
        Self {
            stdout_buffer: String::new(),
            stderr_buffer: String::new(),
            progress_manager,
            indent: " ".repeat(indent_level),
            display_output,
        }
    }

    /// Process a chunk of output - displays and captures it
    pub fn process_chunk(&mut self, chunk: OutputChunk) {
        match chunk {
            OutputChunk::Stdout(line) => {
                // Capture the output
                let _ = writeln!(self.stdout_buffer, "{}", line.trim_end());

                // Display if in verbose mode
                if self.display_output {
                    self.progress_manager.print_verbose(format!(
                        "{}stdout: {}",
                        self.indent,
                        line.trim_end()
                    ));
                }
            }
            OutputChunk::Stderr(line) => {
                // Capture the output
                let _ = writeln!(self.stderr_buffer, "{}", line.trim_end());

                // Display if in verbose mode
                if self.display_output {
                    self.progress_manager.print_warning(format!(
                        "{}stderr: {}",
                        self.indent,
                        line.trim_end()
                    ));
                }
            }
        }
    }

    /// Get the captured stdout content
    pub fn stdout(&self) -> &str {
        &self.stdout_buffer
    }

    /// Get the captured stderr content
    pub fn stderr(&self) -> &str {
        &self.stderr_buffer
    }

    /// Convert to a callback closure that can be passed to execute_streaming
    pub fn into_callback(mut self) -> impl FnMut(OutputChunk) + Send + 'static {
        move |chunk| self.process_chunk(chunk)
    }
}
