//! Excel number format codes (placeholder implementation).

use crate::style::Color;
use crate::value::Value;

/// Formatted cell text plus an optional color from a `[Red]`-style section.
#[derive(Debug, Clone, PartialEq)]
pub struct Formatted {
    pub text: String,
    pub color: Option<Color>,
}

/// Format a value with an Excel number format code.
pub fn format_value(v: &Value, _code: &str) -> Formatted {
    Formatted { text: v.display(), color: None }
}

/// True if the format code displays dates or times.
pub fn is_date_format(code: &str) -> bool {
    let lower = code.to_ascii_lowercase();
    lower.contains('y') || lower.contains('d') || lower.contains("h:") || lower.contains("mm:ss")
}
