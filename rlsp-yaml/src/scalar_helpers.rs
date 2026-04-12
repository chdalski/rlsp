// SPDX-License-Identifier: MIT

//! Scalar type inference helpers for YAML 1.2 Core schema.
//!
//! `rlsp-yaml-parser` represents all scalars as strings. These functions infer
//! the logical type from string content, matching the Core schema rules in
//! `rlsp_yaml_parser::schema::CoreSchema`.

/// The inferred YAML Core schema type of a plain scalar value.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PlainScalarKind {
    /// `null`, `Null`, `NULL`, `~`, or empty string.
    Null,
    /// `true`/`True`/`TRUE` or `false`/`False`/`FALSE`.
    Bool,
    /// Decimal, octal (`0o`), or hexadecimal integer with optional sign.
    Integer,
    /// Floating-point, including `.inf`/`.nan` variants.
    Float,
    /// Any value that does not match the above.
    String,
}

/// Classify a plain (unquoted) scalar by its YAML Core schema type.
#[must_use]
pub fn classify_plain_scalar(value: &str) -> PlainScalarKind {
    if is_null(value) {
        PlainScalarKind::Null
    } else if is_bool(value) {
        PlainScalarKind::Bool
    } else if is_integer(value) {
        PlainScalarKind::Integer
    } else if is_float(value) {
        PlainScalarKind::Float
    } else {
        PlainScalarKind::String
    }
}

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
#[expect(clippy::approx_constant, clippy::unwrap_used, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;

    // ── is_null ───────────────────────────────────────────────────────────────

    #[rstest]
    #[case::lowercase("null")]
    #[case::titlecase("Null")]
    #[case::uppercase("NULL")]
    #[case::tilde("~")]
    #[case::empty("")]
    fn is_null_returns_true(#[case] input: &str) {
        assert!(is_null(input));
    }

    #[rstest]
    #[case::none_string("none")]
    #[case::nil_string("nil")]
    #[case::mixed_case("nUll")]
    #[case::single_space(" ")]
    #[case::double_space("  ")]
    fn is_null_returns_false(#[case] input: &str) {
        assert!(!is_null(input));
    }

    // ── is_bool ───────────────────────────────────────────────────────────────

    #[rstest]
    #[case::true_lowercase("true")]
    #[case::true_titlecase("True")]
    #[case::true_uppercase("TRUE")]
    #[case::false_lowercase("false")]
    #[case::false_titlecase("False")]
    #[case::false_uppercase("FALSE")]
    fn is_bool_returns_true(#[case] input: &str) {
        assert!(is_bool(input));
    }

    #[rstest]
    #[case::yes("yes")]
    #[case::no("no")]
    #[case::on("on")]
    #[case::off("off")]
    #[case::mixed_case("tRue")]
    fn is_bool_returns_false(#[case] input: &str) {
        assert!(!is_bool(input));
    }

    // ── parse_integer ─────────────────────────────────────────────────────────

    #[rstest]
    #[case::decimal_positive("42", 42)]
    #[case::decimal_zero("0", 0)]
    #[case::decimal_negative("-1", -1)]
    #[case::decimal_plus_prefix("+100", 100)]
    #[case::octal("0o17", 15)]
    #[case::octal_negative("-0o10", -8)]
    #[case::hex_uppercase("0xFF", 255)]
    #[case::hex_negative("-0x1A", -26)]
    #[case::hex_lowercase("0xdeadbeef", 0xdead_beef)]
    fn parse_integer_returns_some(#[case] input: &str, #[case] expected: i64) {
        assert_eq!(parse_integer(input), Some(expected));
    }

    #[rstest]
    #[case::leading_zeros_triple("007")]
    #[case::leading_zeros_double("00")]
    #[case::empty_octal_prefix("0o")]
    #[case::empty_hex_prefix("0x")]
    #[case::bare_plus("+")]
    #[case::bare_minus("-")]
    #[case::empty("")]
    fn parse_integer_returns_none(#[case] input: &str) {
        assert_eq!(parse_integer(input), None);
    }

    #[rstest]
    #[case::decimal("42", true)]
    #[case::positive_signed_zero("+0", true)]
    #[case::positive_signed("+42", true)]
    #[case::float_with_dot("3.14", false)]
    #[case::float_looking_dot("1.0", false)]
    #[case::float_looking_exp("1e5", false)]
    #[case::float_looking_exp_dot("1.5e3", false)]
    #[case::alpha("abc", false)]
    #[case::alpha_mixed("1a2", false)]
    fn is_integer_bool_cases(#[case] input: &str, #[case] expected: bool) {
        assert_eq!(is_integer(input), expected);
    }

    // ── parse_float ───────────────────────────────────────────────────────────

    #[rstest]
    #[case::decimal_pi("3.14", 3.14)]
    #[case::decimal_negative("-0.5", -0.5)]
    #[case::decimal_positive_signed("+1.0", 1.0)]
    #[case::exponent("1e10", 1e10)]
    #[case::exponent_negative("1.5E-3", 1.5e-3)]
    #[case::inf_lowercase(".inf", f64::INFINITY)]
    #[case::inf_titlecase(".Inf", f64::INFINITY)]
    #[case::inf_uppercase(".INF", f64::INFINITY)]
    #[case::neg_inf_lowercase("-.inf", f64::NEG_INFINITY)]
    #[case::neg_inf_titlecase("-.Inf", f64::NEG_INFINITY)]
    #[case::neg_inf_uppercase("-.INF", f64::NEG_INFINITY)]
    fn parse_float_returns_value(#[case] input: &str, #[case] expected: f64) {
        assert_eq!(parse_float(input), Some(expected));
    }

    #[rstest]
    #[case::nan_lowercase(".nan")]
    #[case::nan_titlecase(".NaN")]
    #[case::nan_uppercase(".NAN")]
    fn parse_float_returns_nan(#[case] input: &str) {
        assert!(parse_float(input).unwrap().is_nan());
    }

    #[rstest]
    #[case::integer("42")]
    #[case::alpha("abc")]
    #[case::empty("")]
    fn parse_float_returns_none(#[case] input: &str) {
        assert_eq!(parse_float(input), None);
    }

    #[rstest]
    #[case::decimal("3.14", true)]
    #[case::inf(".inf", true)]
    #[case::integer("42", false)]
    #[case::bare_inf("inf", false)]
    #[case::bare_nan("nan", false)]
    fn is_float_bool_cases(#[case] input: &str, #[case] expected: bool) {
        assert_eq!(is_float(input), expected);
    }
}
