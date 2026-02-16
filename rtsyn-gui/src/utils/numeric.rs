use eframe::egui;

/// Truncates a floating-point number to 6 decimal places.
///
/// This function provides consistent precision limiting for display purposes,
/// preventing excessive decimal places in the UI while maintaining reasonable accuracy.
///
/// # Arguments
/// * `value` - The floating-point number to truncate
///
/// # Returns
/// The value truncated to 6 decimal places
///
/// # Example
/// ```rust
/// assert_eq!(truncate_f64(1.23456789), 1.234567);
/// assert_eq!(truncate_f64(1.0), 1.0);
/// ```
pub fn truncate_f64(value: f64) -> f64 {
    (value * 1_000_000.0).trunc() / 1_000_000.0
}

/// Formats a floating-point number with up to 6 decimal places, removing trailing zeros.
///
/// This function provides clean number formatting for UI display by truncating
/// to 6 decimal places and removing unnecessary trailing zeros and decimal points.
///
/// # Arguments
/// * `value` - The floating-point number to format
///
/// # Returns
/// A formatted string representation without trailing zeros
///
/// # Examples
/// ```rust
/// assert_eq!(format_f64_6(1.0), "1");
/// assert_eq!(format_f64_6(1.2345), "1.2345");
/// assert_eq!(format_f64_6(1.234500), "1.2345");
/// ```
pub fn format_f64_6(value: f64) -> String {
    let truncated = truncate_f64(value);
    let mut text = format!("{:.6}", truncated);
    while text.contains('.') && text.ends_with('0') {
        text.pop();
    }
    if text.ends_with('.') {
        text.pop();
    }
    text
}

/// Parses user input text into a floating-point number with locale support.
///
/// This function handles various input formats and edge cases commonly
/// encountered in user interfaces, including comma decimal separators
/// and incomplete input states.
///
/// # Arguments
/// * `text` - The input text to parse
///
/// # Returns
/// `Some(f64)` if parsing succeeds, `None` for invalid or incomplete input
///
/// # Supported Formats
/// - Standard decimal notation: "1.25", "2.5"
/// - Comma decimal separator: "1,25" (converted to "1.25")
/// - Negative numbers: "-1.5"
///
/// # Invalid Cases
/// - Empty strings or whitespace-only
/// - Incomplete input: "-", "1.", "1,"
/// - Non-numeric characters (except decimal separators)
///
/// # Examples
/// ```rust
/// assert_eq!(parse_f64_input("1.25"), Some(1.25));
/// assert_eq!(parse_f64_input("2,5"), Some(2.5));
/// assert_eq!(parse_f64_input(""), None);
/// assert_eq!(parse_f64_input("-"), None);
/// ```
pub fn parse_f64_input(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.ends_with('.') || trimmed.ends_with(',') {
        return None;
    }
    let normalized = trimmed.replace(',', ".");
    normalized.parse::<f64>().ok()
}

/// Formats a floating-point number while preserving user input formatting preferences.
///
/// This function maintains the user's intended decimal precision and formatting
/// style (like trailing zeros) while ensuring the result stays within reasonable
/// bounds for display purposes.
///
/// # Arguments
/// * `buffer` - The user's input text buffer
/// * `value` - The actual numeric value to format
///
/// # Returns
/// A formatted string that respects user input style when possible
///
/// # Behavior
/// - Preserves trailing zeros if present in user input
/// - Limits fractional part to 6 digits maximum
/// - Falls back to standard formatting if input is not decimal format
/// - Handles comma decimal separators by converting to dots
///
/// # Examples
/// ```rust
/// assert_eq!(format_f64_with_input("1.0", 1.0), "1.0");
/// assert_eq!(format_f64_with_input("2,50", 2.5), "2.50");
/// assert_eq!(format_f64_with_input("3.14159265", 3.141592), "3.141592");
/// ```
pub fn format_f64_with_input(buffer: &str, value: f64) -> String {
    let normalized = buffer.trim().replace(',', ".");
    if let Some((int_part, frac_part)) = normalized.split_once('.') {
        let mut frac = frac_part.to_string();
        if frac.len() > 6 {
            frac.truncate(6);
        }
        if frac.is_empty() {
            int_part.to_string()
        } else {
            format!("{int_part}.{frac}")
        }
    } else {
        format_f64_6(value)
    }
}

/// Normalizes user input to contain only valid numeric characters.
///
/// This function filters input text to allow only digits, one decimal separator,
/// and one leading minus sign, providing real-time input validation for
/// numeric text fields.
///
/// # Arguments
/// * `buffer` - Mutable string buffer to normalize in-place
///
/// # Returns
/// `true` if the buffer was modified, `false` if no changes were needed
///
/// # Normalization Rules
/// - Allows digits (0-9)
/// - Allows one decimal separator (. or ,)
/// - Allows one minus sign at the beginning only
/// - Removes all other characters
/// - Prevents multiple decimal separators or minus signs
///
/// # Examples
/// ```rust
/// let mut input = "1.2.3".to_string();
/// assert!(normalize_numeric_input(&mut input));
/// assert_eq!(input, "1.23");
///
/// let mut input = "-0,0,1".to_string();
/// assert!(normalize_numeric_input(&mut input));
/// assert_eq!(input, "-0,01");
/// ```
pub fn normalize_numeric_input(buffer: &mut String) -> bool {
    let mut out = String::with_capacity(buffer.len());
    let mut seen_sep = false;
    let mut seen_sign = false;
    for ch in buffer.chars() {
        if ch.is_ascii_digit() {
            out.push(ch);
        } else if (ch == '.' || ch == ',') && !seen_sep {
            out.push(ch);
            seen_sep = true;
        } else if ch == '-' && !seen_sign && out.is_empty() {
            out.push(ch);
            seen_sign = true;
        }
    }
    if out != *buffer {
        *buffer = out;
        true
    } else {
        false
    }
}

/// Calculates the shortest distance from a point to a line segment.
///
/// This function computes the perpendicular distance from a point to the
/// closest point on a line segment, useful for hit-testing and proximity
/// calculations in graphical interfaces.
///
/// # Arguments
/// * `point` - The point to measure distance from
/// * `a` - First endpoint of the line segment
/// * `b` - Second endpoint of the line segment
///
/// # Returns
/// The shortest distance from the point to the line segment
///
/// # Algorithm
/// - Projects the point onto the infinite line through the segment
/// - Clamps the projection to the segment endpoints
/// - Returns the distance to the clamped point
/// - Handles degenerate cases where the segment has zero length
///
/// # Example
/// ```rust
/// let point = egui::pos2(5.0, 1.0);
/// let a = egui::pos2(0.0, 0.0);
/// let b = egui::pos2(10.0, 0.0);
/// let distance = distance_to_segment(point, a, b);
/// assert_eq!(distance, 1.0); // Point is 1 unit above the segment
/// ```
pub fn distance_to_segment(point: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
    let ab = b - a;
    let ap = point - a;
    let ab_len_sq = ab.x * ab.x + ab.y * ab.y;
    if ab_len_sq == 0.0 {
        return ap.length();
    }
    let t = ((ap.x * ab.x) + (ap.y * ab.y)) / ab_len_sq;
    let t = t.clamp(0.0, 1.0);
    let closest = a + ab * t;
    (point - closest).length()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_and_format_do_not_pad_trailing_zeros() {
        assert_eq!(truncate_f64(1.23456789), 1.234567);
        assert_eq!(format_f64_6(1.0), "1");
        assert_eq!(format_f64_6(1.2345), "1.2345");
    }

    #[test]
    fn parse_f64_input_accepts_commas() {
        assert_eq!(parse_f64_input("1,25"), Some(1.25));
        assert_eq!(parse_f64_input("2.5"), Some(2.5));
        assert_eq!(parse_f64_input(""), None);
    }

    #[test]
    fn format_f64_with_input_preserves_trailing_zeros() {
        assert_eq!(format_f64_with_input("1.0", 1.0), "1.0");
        assert_eq!(format_f64_with_input("2,50", 2.5), "2.50");
        assert_eq!(format_f64_with_input("3.14159265", 3.141592), "3.141592");
        assert_eq!(format_f64_with_input("4", 4.0), "4");
    }

    #[test]
    fn normalize_numeric_input_rejects_multiple_separators() {
        let mut value = "1.2.3".to_string();
        assert!(normalize_numeric_input(&mut value));
        assert_eq!(value, "1.23");
        let mut value = "-0,0,1".to_string();
        assert!(normalize_numeric_input(&mut value));
        assert_eq!(value, "-0,01");
    }

    #[test]
    fn distance_to_segment_returns_zero_on_segment() {
        let a = egui::pos2(0.0, 0.0);
        let b = egui::pos2(10.0, 0.0);
        let p = egui::pos2(5.0, 0.0);
        assert!(distance_to_segment(p, a, b) < 1e-6);
    }
}
