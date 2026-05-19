// SPDX-License-Identifier: MIT

//! Value-parsing and YAML 1.1 compatibility helpers.
//!
//! `rlsp_yaml_parser` resolves tags and exposes them as URIs; callers use those
//! URIs for type identification.  This module handles two remaining concerns:
//! (1) value extraction for JSON Schema range validation (`parse_integer`,
//! `parse_float`), and (2) YAML 1.1 compatibility warnings (`is_yaml11_bool`,
//! `yaml11_bool_canonical`, `is_yaml11_octal`).

/// Returns `true` if the value is a YAML 1.1 boolean form that is NOT a boolean in YAML 1.2.
///
/// The 16 forms: `yes`, `Yes`, `YES`, `no`, `No`, `NO`, `on`, `On`, `ON`,
/// `off`, `Off`, `OFF`, `y`, `Y`, `n`, `N`.
#[must_use]
pub fn is_yaml11_bool(value: &str) -> bool {
    matches!(
        value,
        "yes"
            | "Yes"
            | "YES"
            | "no"
            | "No"
            | "NO"
            | "on"
            | "On"
            | "ON"
            | "off"
            | "Off"
            | "OFF"
            | "y"
            | "Y"
            | "n"
            | "N"
    )
}

/// Returns `true` if the value is a C-style octal literal as parsed by YAML 1.1.
///
/// Matches strings that start with `0`, have length > 1, and contain only
/// digits `0`–`7`. The single character `"0"` is a valid YAML 1.2 integer and
/// is excluded.
#[must_use]
pub fn is_yaml11_octal(value: &str) -> bool {
    if value.len() <= 1 {
        return false;
    }
    let mut chars = value.chars();
    if chars.next() != Some('0') {
        return false;
    }
    chars.all(|c| matches!(c, '0'..='7'))
}

/// Map a YAML 1.1 boolean form to its canonical YAML 1.2 equivalent.
///
/// Returns `"true"` for the true-canonical forms (`yes`, `Yes`, `YES`, `on`,
/// `On`, `ON`, `y`, `Y`) and `"false"` for the false-canonical forms (`no`,
/// `No`, `NO`, `off`, `Off`, `OFF`, `n`, `N`).
///
/// The caller is responsible for ensuring `value` is a valid YAML 1.1 boolean
/// (i.e. `is_yaml11_bool(value)` returns `true`).
#[must_use]
pub fn yaml11_bool_canonical(value: &str) -> &'static str {
    match value {
        "yes" | "Yes" | "YES" | "on" | "On" | "ON" | "y" | "Y" => "true",
        _ => "false",
    }
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
#[expect(clippy::approx_constant, reason = "test code")]
mod tests {
    use rstest::rstest;

    use super::*;

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

    // ── is_yaml11_bool ────────────────────────────────────────────────────────

    #[rstest]
    #[case::yes_lowercase("yes")]
    #[case::yes_titlecase("Yes")]
    #[case::yes_uppercase("YES")]
    #[case::no_lowercase("no")]
    #[case::no_titlecase("No")]
    #[case::no_uppercase("NO")]
    #[case::on_lowercase("on")]
    #[case::on_titlecase("On")]
    #[case::on_uppercase("ON")]
    #[case::off_lowercase("off")]
    #[case::off_titlecase("Off")]
    #[case::off_uppercase("OFF")]
    #[case::y_lowercase("y")]
    #[case::y_uppercase("Y")]
    #[case::n_lowercase("n")]
    #[case::n_uppercase("N")]
    fn is_yaml11_bool_returns_true(#[case] input: &str) {
        assert!(is_yaml11_bool(input));
    }

    #[rstest]
    #[case::yaml12_true_lowercase("true")]
    #[case::yaml12_true_titlecase("True")]
    #[case::yaml12_true_uppercase("TRUE")]
    #[case::yaml12_false_lowercase("false")]
    #[case::yaml12_false_titlecase("False")]
    #[case::yaml12_false_uppercase("FALSE")]
    #[case::empty("")]
    #[case::mixed_case_yes("yEs")]
    #[case::mixed_case_no("nO")]
    #[case::prefix_yes("yess")]
    #[case::suffix_no("noo")]
    #[case::space_padded(" yes")]
    #[case::number_zero("0")]
    #[case::number_one("1")]
    fn is_yaml11_bool_returns_false(#[case] input: &str) {
        assert!(!is_yaml11_bool(input));
    }

    // ── is_yaml11_octal ───────────────────────────────────────────────────────

    #[rstest]
    #[case::two_digits_leading_zero("01")]
    #[case::three_digits("007")]
    #[case::max_octal_digit("077")]
    #[case::longer_value("0755")]
    #[case::larger_value("01234567")]
    fn is_yaml11_octal_returns_true(#[case] input: &str) {
        assert!(is_yaml11_octal(input));
    }

    #[rstest]
    #[case::zero_alone("0")]
    #[case::yaml12_octal("0o17")]
    #[case::decimal_no_leading_zero("42")]
    #[case::has_digit_eight("08")]
    #[case::has_digit_nine("09")]
    #[case::leading_zero_with_alpha("0x10")]
    #[case::signed_octal("-07")]
    #[case::empty("")]
    #[case::just_letters("abc")]
    #[case::embedded_space("0 7")]
    #[case::leading_whitespace(" 07")]
    fn is_yaml11_octal_returns_false(#[case] input: &str) {
        assert!(!is_yaml11_octal(input));
    }

    // ── yaml11_bool_canonical ─────────────────────────────────────────────────

    #[rstest]
    #[case::yes_lowercase("yes")]
    #[case::yes_titlecase("Yes")]
    #[case::yes_uppercase("YES")]
    #[case::on_lowercase("on")]
    #[case::on_titlecase("On")]
    #[case::on_uppercase("ON")]
    #[case::y_lowercase("y")]
    #[case::y_uppercase("Y")]
    fn yaml11_bool_canonical_returns_true(#[case] input: &str) {
        assert_eq!(yaml11_bool_canonical(input), "true");
    }

    #[rstest]
    #[case::no_lowercase("no")]
    #[case::no_titlecase("No")]
    #[case::no_uppercase("NO")]
    #[case::off_lowercase("off")]
    #[case::off_titlecase("Off")]
    #[case::off_uppercase("OFF")]
    #[case::n_lowercase("n")]
    #[case::n_uppercase("N")]
    fn yaml11_bool_canonical_returns_false(#[case] input: &str) {
        assert_eq!(yaml11_bool_canonical(input), "false");
    }
}
