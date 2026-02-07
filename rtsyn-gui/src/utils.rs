use eframe::egui;

pub(crate) fn truncate_f64(value: f64) -> f64 {
    (value * 1_000_000.0).trunc() / 1_000_000.0
}

pub(crate) fn format_f64_6(value: f64) -> String {
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

pub(crate) fn parse_f64_input(text: &str) -> Option<f64> {
    let trimmed = text.trim();
    if trimmed.is_empty() || trimmed == "-" || trimmed.ends_with('.') || trimmed.ends_with(',') {
        return None;
    }
    let normalized = trimmed.replace(',', ".");
    normalized.parse::<f64>().ok()
}

pub(crate) fn format_f64_with_input(buffer: &str, value: f64) -> String {
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

pub(crate) fn normalize_numeric_input(buffer: &mut String) -> bool {
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

pub(crate) fn distance_to_segment(point: egui::Pos2, a: egui::Pos2, b: egui::Pos2) -> f32 {
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
