/// Formats a numeric value for display with appropriate precision.
///
/// This function provides consistent number formatting across the application,
/// using integer formatting for whole numbers and 4 decimal places for
/// fractional values.
///
/// # Arguments
/// * `value` - The numeric value to format
///
/// # Returns
/// A formatted string representation of the number
///
/// # Formatting Rules
/// - Whole numbers (fractional part ≈ 0): displayed without decimal places
/// - Fractional numbers: displayed with up to 4 decimal places
/// - Uses floating-point epsilon for whole number detection
///
/// # Examples
/// ```rust
/// assert_eq!(format_number_value(42.0), "42");
/// assert_eq!(format_number_value(3.14159), "3.1416");
/// assert_eq!(format_number_value(1.0000001), "1"); // Close to whole number
/// ```
pub fn format_number_value(value: f64) -> String {
    if value.fract().abs() < f64::EPSILON {
        format!("{value:.0}")
    } else {
        format!("{value:.4}")
    }
}