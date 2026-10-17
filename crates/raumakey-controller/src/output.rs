//! Where commands print: standard output for results, standard error for errors.
//!
//! Commands never print keystrokes, document or clipboard text, or macro text. They print file
//! paths only when the user passed them or when they name Raumakey's own files.

use core::fmt;
use std::io::Write;

/// Prefix of error lines.
const PREFIX: &str = "raumakey: ";

/// The two output streams of a command. Write errors are ignored: a closed console or pipe must
/// not change what a command does.
pub struct Output<'a> {
    out: &'a mut dyn Write,
    err: &'a mut dyn Write,
}

impl fmt::Debug for Output<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Output").finish_non_exhaustive()
    }
}

impl<'a> Output<'a> {
    /// Output to `out` (standard output) and `err` (standard error).
    pub fn new(out: &'a mut dyn Write, err: &'a mut dyn Write) -> Self {
        Self { out, err }
    }

    /// One line on standard output.
    pub fn line(&mut self, text: impl fmt::Display) {
        let _ = writeln!(self.out, "{text}");
    }

    /// Text on standard output, as is.
    pub fn text(&mut self, text: &str) {
        let _ = self.out.write_all(text.as_bytes());
    }

    /// An error line on standard error: `raumakey: <text>`.
    pub fn error(&mut self, text: impl fmt::Display) {
        let _ = writeln!(self.err, "{PREFIX}{text}");
    }

    /// A line on standard error that continues an error (indented, no prefix).
    pub fn detail(&mut self, text: impl fmt::Display) {
        let _ = writeln!(self.err, "  {text}");
    }

    /// Text on standard error, as is.
    pub fn error_text(&mut self, text: &str) {
        let _ = self.err.write_all(text.as_bytes());
    }

    /// Flushes both streams.
    pub fn flush(&mut self) {
        let _ = self.out.flush();
        let _ = self.err.flush();
    }
}
