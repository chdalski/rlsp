// SPDX-License-Identifier: MIT

//! Scalar type inference helpers for YAML 1.2 Core schema.
//!
//! `rlsp-yaml-parser` represents all scalars as strings. These functions infer
//! the logical type from string content, matching the Core schema rules in
//! `rlsp_yaml_parser::schema::CoreSchema`.

/// Returns `true` if the value represents a YAML null.
///
/// Matches: `null`, `Null`, `NULL`, `~`, empty string.
#[must_use]
pub fn is_null(value: &str) -> bool {
    matches!(value, "null" | "Null" | "NULL" | "~" | "")
}

/// Returns `true` if the value represents a YAML boolean.
///
/// Matches: `true`/`True`/`TRUE`, `false`/`False`/`FALSE`.
#[must_use]
pub fn is_bool(value: &str) -> bool {
    matches!(
        value,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE"
    )
}

/// Returns `true` if the value represents a YAML integer.
///
/// Supports decimal, octal (`0o`), and hex (`0x`) with optional `+`/`-` prefix.
#[must_use]
pub fn is_integer(value: &str) -> bool {
    parse_integer(value).is_some()
}

/// Returns `true` if the value represents a YAML float.
///
/// Supports decimal with `.`, exponent notation, `.inf`/`.nan` variants.
#[must_use]
pub fn is_float(value: &str) -> bool {
    parse_float(value).is_some()
}

/// Parse a YAML Core schema integer from a string value.
///
/// Supports decimal, octal (`0o`), and hex (`0x`) with optional `+`/`-` prefix.
/// Leading zeros in decimal (e.g. `007`) are not valid.
#[must_use]
pub fn parse_integer(value: &str) -> Option<i64> {
    let (neg, rest) = value.strip_prefix('-').map_or_else(
        || (false, value.strip_prefix('+').unwrap_or(value)),
        |r| (true, r),
    );
    if rest.is_empty() {
        return None;
    }
    let magnitude: i64 = if let Some(oct) = rest.strip_prefix("0o") {
        if oct.is_empty() {
            return None;
        }
        i64::from_str_radix(oct, 8).ok()?
    } else if let Some(hex) = rest.strip_prefix("0x") {
        if hex.is_empty() {
            return None;
        }
        i64::from_str_radix(hex, 16).ok()?
    } else {
        if rest.len() > 1 && rest.starts_with('0') {
            return None;
        }
        if !rest.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        rest.parse::<i64>().ok()?
    };
    Some(if neg { -magnitude } else { magnitude })
}

/// Parse a YAML Core schema float from a string value.
///
/// Supports decimal with `.`, exponent notation (`e`/`E`), and special values
/// `.inf`/`.Inf`/`.INF`, `-.inf`/`-.Inf`/`-.INF`, `.nan`/`.NaN`/`.NAN`.
#[must_use]
pub fn parse_float(value: &str) -> Option<f64> {
    match value {
        ".inf" | ".Inf" | ".INF" => return Some(f64::INFINITY),
        "-.inf" | "-.Inf" | "-.INF" => return Some(f64::NEG_INFINITY),
        ".nan" | ".NaN" | ".NAN" => return Some(f64::NAN),
        _ => {}
    }
    let stripped = value.strip_prefix('+').unwrap_or(value);
    let signed = stripped.strip_prefix('-').unwrap_or(stripped);
    if signed.contains('.') || signed.contains('e') || signed.contains('E') {
        return value.trim_start_matches('+').parse::<f64>().ok();
    }
    None
}

#[cfg(test)]
#[allow(clippy::approx_constant, clippy::float_cmp, clippy::unwrap_used)]
mod tests {
    use super::*;

    // is_null

    #[test]
    fn null_lowercase() {
        assert!(is_null("null"));
    }

    #[test]
    fn null_titlecase() {
        assert!(is_null("Null"));
    }

    #[test]
    fn null_uppercase() {
        assert!(is_null("NULL"));
    }

    #[test]
    fn null_tilde() {
        assert!(is_null("~"));
    }

    #[test]
    fn null_empty() {
        assert!(is_null(""));
    }

    #[test]
    fn not_null_string() {
        assert!(!is_null("none"));
        assert!(!is_null("nil"));
        assert!(!is_null("nUll"));
    }

    // is_bool

    #[test]
    fn bool_true_variants() {
        assert!(is_bool("true"));
        assert!(is_bool("True"));
        assert!(is_bool("TRUE"));
    }

    #[test]
    fn bool_false_variants() {
        assert!(is_bool("false"));
        assert!(is_bool("False"));
        assert!(is_bool("FALSE"));
    }

    #[test]
    fn not_bool() {
        assert!(!is_bool("yes"));
        assert!(!is_bool("no"));
        assert!(!is_bool("on"));
        assert!(!is_bool("off"));
        assert!(!is_bool("tRue"));
    }

    // is_integer / parse_integer

    #[test]
    fn integer_decimal() {
        assert_eq!(parse_integer("42"), Some(42));
        assert_eq!(parse_integer("0"), Some(0));
        assert_eq!(parse_integer("-1"), Some(-1));
        assert_eq!(parse_integer("+100"), Some(100));
    }

    #[test]
    fn integer_octal() {
        assert_eq!(parse_integer("0o17"), Some(15));
        assert_eq!(parse_integer("-0o10"), Some(-8));
    }

    #[test]
    fn integer_hex() {
        assert_eq!(parse_integer("0xFF"), Some(255));
        assert_eq!(parse_integer("-0x1A"), Some(-26));
    }

    #[test]
    fn integer_leading_zeros_rejected() {
        assert_eq!(parse_integer("007"), None);
        assert_eq!(parse_integer("00"), None);
    }

    #[test]
    fn integer_empty_prefix_rejected() {
        assert_eq!(parse_integer("0o"), None);
        assert_eq!(parse_integer("0x"), None);
        assert_eq!(parse_integer("+"), None);
        assert_eq!(parse_integer("-"), None);
        assert_eq!(parse_integer(""), None);
    }

    #[test]
    fn is_integer_delegates_to_parse() {
        assert!(is_integer("42"));
        assert!(!is_integer("3.14"));
        assert!(!is_integer("abc"));
    }

    // is_float / parse_float

    #[test]
    fn float_decimal() {
        assert_eq!(parse_float("3.14"), Some(3.14));
        assert_eq!(parse_float("-0.5"), Some(-0.5));
        assert_eq!(parse_float("+1.0"), Some(1.0));
    }

    #[test]
    fn float_exponent() {
        assert_eq!(parse_float("1e10"), Some(1e10));
        assert_eq!(parse_float("1.5E-3"), Some(1.5e-3));
    }

    #[test]
    fn float_inf() {
        assert_eq!(parse_float(".inf"), Some(f64::INFINITY));
        assert_eq!(parse_float(".Inf"), Some(f64::INFINITY));
        assert_eq!(parse_float(".INF"), Some(f64::INFINITY));
        assert_eq!(parse_float("-.inf"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_float("-.Inf"), Some(f64::NEG_INFINITY));
        assert_eq!(parse_float("-.INF"), Some(f64::NEG_INFINITY));
    }

    #[test]
    fn float_nan() {
        assert!(parse_float(".nan").unwrap().is_nan());
        assert!(parse_float(".NaN").unwrap().is_nan());
        assert!(parse_float(".NAN").unwrap().is_nan());
    }

    #[test]
    fn not_float() {
        assert_eq!(parse_float("42"), None);
        assert_eq!(parse_float("abc"), None);
        assert_eq!(parse_float(""), None);
    }

    #[test]
    fn is_float_delegates_to_parse() {
        assert!(is_float("3.14"));
        assert!(is_float(".inf"));
        assert!(!is_float("42"));
    }

    // TE tests 7, 13, 19, 20, 27, 33, 41

    #[test]
    fn is_null_returns_false_for_whitespace() {
        assert!(!is_null(" "));
        assert!(!is_null("  "));
    }

    #[test]
    fn is_integer_returns_true_for_positive_signed() {
        assert!(is_integer("+0"));
        assert!(is_integer("+42"));
    }

    #[test]
    fn is_integer_returns_false_for_float_looking_strings() {
        assert!(!is_integer("1.0"));
        assert!(!is_integer("1e5"));
        assert!(!is_integer("1.5e3"));
    }

    #[test]
    fn is_integer_returns_false_for_non_numeric_with_letters() {
        assert!(!is_integer("abc"));
        assert!(!is_integer("1a2"));
    }

    #[test]
    fn is_float_returns_false_for_bare_inf_and_nan() {
        assert!(!is_float("inf"));
        assert!(!is_float("nan"));
    }

    #[test]
    fn parse_integer_hex_lowercase() {
        assert_eq!(parse_integer("0xdeadbeef"), Some(0xdead_beef));
    }

    #[test]
    fn parse_float_positive_signed() {
        assert_eq!(parse_float("+1.0"), Some(1.0));
    }
}
