//! String manipulation utilities for the RTSyn GUI.

/// Truncates a string to a maximum character count, adding ellipsis if truncated.
///
/// This function intelligently truncates strings at character boundaries,
/// ensuring Unicode safety. If the string exceeds the maximum length,
/// it's truncated and "..." is appended.
///
/// # Parameters
/// - `s`: The string to truncate
/// - `max_chars`: Maximum number of characters before truncation
///
/// # Returns
/// A new String that is either the original (if short enough) or truncated with "..."
///
/// # Examples
/// ```
/// let short = truncate_string("Hello", 10);
/// assert_eq!(short, "Hello");
///
/// let long = truncate_string("Hello World", 8);
/// assert_eq!(long, "Hello...");
/// ```
pub fn truncate_string(s: &str, max_chars: usize) -> String {
    if s.chars().count() <= max_chars {
        s.to_string()
    } else {
        let truncated: String = s.chars().take(max_chars.saturating_sub(3)).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_string_preserves_short_strings() {
        assert_eq!(truncate_string("Hello", 10), "Hello");
        assert_eq!(truncate_string("Test", 4), "Test");
    }

    #[test]
    fn truncate_string_adds_ellipsis() {
        assert_eq!(truncate_string("Hello World", 8), "Hello...");
        assert_eq!(truncate_string("1234567890", 7), "1234...");
    }

    #[test]
    fn truncate_string_handles_unicode() {
        assert_eq!(truncate_string("Hello 世界", 8), "Hello...");
        assert_eq!(truncate_string("世界", 5), "世界");
    }
}
