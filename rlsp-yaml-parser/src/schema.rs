// SPDX-License-Identifier: MIT

//! YAML schema resolution — tag resolution and scalar type inference.
//!
//! Three standard schemas are provided:
//! - [`FailsafeSchema`] — all scalars are strings.
//! - [`JsonSchema`] — strict JSON-compatible type inference.
//! - [`CoreSchema`] — YAML 1.2 Core schema (default); extends JSON with
//!   additional null/bool/int/float patterns and octal/hex integer literals.
//!
//! The [`Schema`] trait is object-safe; callers may supply a custom
//! implementation via `&dyn Schema`.

use crate::event::ScalarStyle;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// A fully resolved scalar value.
///
/// Note: `Scalar` intentionally does not implement `Eq` because `f64` has
/// `NaN != NaN` semantics.  Use `.is_nan()` for NaN comparisons and
/// `(a - b).abs() < eps` for finite float comparisons.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    /// A null value.
    Null,
    /// A boolean value.
    Bool(bool),
    /// An integer value.
    Int(i64),
    /// A floating-point value (including ±infinity and NaN).
    Float(f64),
    /// A string value.
    String(String),
}

/// A resolved YAML tag — the result of expanding a raw tag string.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedTag {
    /// `tag:yaml.org,2002:str`
    Str,
    /// `tag:yaml.org,2002:int`
    Int,
    /// `tag:yaml.org,2002:float`
    Float,
    /// `tag:yaml.org,2002:bool`
    Bool,
    /// `tag:yaml.org,2002:null`
    Null,
    /// `tag:yaml.org,2002:seq`
    Seq,
    /// `tag:yaml.org,2002:map`
    Map,
    /// Any tag not recognized by the schema.
    Unknown(String),
}

// ---------------------------------------------------------------------------
// Schema trait
// ---------------------------------------------------------------------------

/// Pluggable schema resolution strategy.
///
/// Implementors define how untagged scalars are resolved and how tag strings
/// are interpreted.  `resolve_scalar` receives the already-expanded tag (if
/// any); shorthand expansion (`!!str` → `tag:yaml.org,2002:str`) is the
/// caller's responsibility.
pub trait Schema {
    /// Resolve a scalar to a typed value.
    ///
    /// - `value` — the scalar text after all YAML escaping has been applied.
    /// - `tag` — the expanded tag, or `None` if the scalar is untagged.
    ///   `Some("!")` means the YAML non-specific tag (forces string).
    ///   `Some("?")` means the untagged marker (schema inference applies).
    /// - `style` — the presentation style; non-plain styles skip type inference
    ///   in JSON and Core schemas.
    fn resolve_scalar(&self, value: &str, tag: Option<&str>, style: ScalarStyle) -> Scalar;

    /// Classify a raw tag string into a [`ResolvedTag`].
    fn resolve_tag(&self, tag: &str) -> ResolvedTag;
}

// ---------------------------------------------------------------------------
// FailsafeSchema
// ---------------------------------------------------------------------------

/// YAML 1.2 Failsafe schema: all scalars are strings.
///
/// Tags are recognized but do not change resolution — every scalar becomes
/// `Scalar::String`.
pub struct FailsafeSchema;

impl Schema for FailsafeSchema {
    fn resolve_scalar(&self, value: &str, _tag: Option<&str>, _style: ScalarStyle) -> Scalar {
        Scalar::String(value.to_owned())
    }

    fn resolve_tag(&self, tag: &str) -> ResolvedTag {
        core_resolve_tag(tag)
    }
}

// ---------------------------------------------------------------------------
// JsonSchema
// ---------------------------------------------------------------------------

/// YAML 1.2 JSON schema.
///
/// Stricter than Core: only lowercase `null`/`true`/`false`, decimal integers
/// only, no octal/hex, no case variants for infinity/NaN.
pub struct JsonSchema;

impl Schema for JsonSchema {
    fn resolve_scalar(&self, value: &str, tag: Option<&str>, style: ScalarStyle) -> Scalar {
        // Explicit tags override inference.
        if let Some(resolved) = apply_explicit_tag(tag, value, self) {
            return resolved;
        }
        // Quoted styles always produce strings in JSON schema.
        if !is_plain(style) {
            return Scalar::String(value.to_owned());
        }
        json_infer(value)
    }

    fn resolve_tag(&self, tag: &str) -> ResolvedTag {
        core_resolve_tag(tag)
    }
}

// ---------------------------------------------------------------------------
// CoreSchema
// ---------------------------------------------------------------------------

/// YAML 1.2 Core schema (default).
///
/// Extends JSON with additional null/bool patterns, octal and hex integer
/// literals, and case-variant infinity/NaN literals.
pub struct CoreSchema;

impl Schema for CoreSchema {
    fn resolve_scalar(&self, value: &str, tag: Option<&str>, style: ScalarStyle) -> Scalar {
        // Explicit tags override inference.
        if let Some(resolved) = apply_explicit_tag(tag, value, self) {
            return resolved;
        }
        // Quoted/block styles always produce strings in Core schema.
        if !is_plain(style) {
            return Scalar::String(value.to_owned());
        }
        core_infer(value)
    }

    fn resolve_tag(&self, tag: &str) -> ResolvedTag {
        core_resolve_tag(tag)
    }
}

// ---------------------------------------------------------------------------
// Internal helpers
// ---------------------------------------------------------------------------

/// Returns `true` if the style is plain (unquoted).
const fn is_plain(style: ScalarStyle) -> bool {
    matches!(style, ScalarStyle::Plain)
}

/// Apply an explicit tag override, if the tag is present and recognized.
///
/// Returns `None` if the tag should be ignored and inference should proceed.
fn apply_explicit_tag(tag: Option<&str>, value: &str, schema: &dyn Schema) -> Option<Scalar> {
    match tag {
        None | Some("?") => None,
        Some("!") => Some(Scalar::String(value.to_owned())),
        Some(t) => match schema.resolve_tag(t) {
            ResolvedTag::Str | ResolvedTag::Unknown(_) => Some(Scalar::String(value.to_owned())),
            ResolvedTag::Null => Some(Scalar::Null),
            ResolvedTag::Bool => Some(core_infer_bool(value).unwrap_or(Scalar::Bool(false))),
            ResolvedTag::Int => Some(core_infer_int(value).unwrap_or(Scalar::Int(0))),
            ResolvedTag::Float => Some(core_infer_float(value).unwrap_or(Scalar::Float(0.0))),
            ResolvedTag::Seq | ResolvedTag::Map => None,
        },
    }
}

/// Resolve a raw tag string to a [`ResolvedTag`].
fn core_resolve_tag(tag: &str) -> ResolvedTag {
    match tag {
        "tag:yaml.org,2002:str" => ResolvedTag::Str,
        "tag:yaml.org,2002:int" => ResolvedTag::Int,
        "tag:yaml.org,2002:float" => ResolvedTag::Float,
        "tag:yaml.org,2002:bool" => ResolvedTag::Bool,
        "tag:yaml.org,2002:null" => ResolvedTag::Null,
        "tag:yaml.org,2002:seq" => ResolvedTag::Seq,
        "tag:yaml.org,2002:map" => ResolvedTag::Map,
        other => ResolvedTag::Unknown(other.to_owned()),
    }
}

/// JSON schema inference for a plain scalar.
fn json_infer(value: &str) -> Scalar {
    if value == "null" {
        return Scalar::Null;
    }
    if let Some(b) = json_infer_bool(value) {
        return b;
    }
    if let Some(i) = json_infer_int(value) {
        return i;
    }
    if let Some(f) = json_infer_float(value) {
        return f;
    }
    Scalar::String(value.to_owned())
}

/// Core schema inference for a plain scalar.
fn core_infer(value: &str) -> Scalar {
    if matches!(value, "null" | "Null" | "NULL" | "~" | "") {
        return Scalar::Null;
    }
    if let Some(b) = core_infer_bool(value) {
        return b;
    }
    if let Some(i) = core_infer_int(value) {
        return i;
    }
    if let Some(f) = core_infer_float(value) {
        return f;
    }
    Scalar::String(value.to_owned())
}

// JSON bool: only `true` / `false` (lowercase).
fn json_infer_bool(value: &str) -> Option<Scalar> {
    match value {
        "true" => Some(Scalar::Bool(true)),
        "false" => Some(Scalar::Bool(false)),
        _ => None,
    }
}

// Core bool: true/false/True/False/TRUE/FALSE.
fn core_infer_bool(value: &str) -> Option<Scalar> {
    match value {
        "true" | "True" | "TRUE" => Some(Scalar::Bool(true)),
        "false" | "False" | "FALSE" => Some(Scalar::Bool(false)),
        _ => None,
    }
}

// JSON int: decimal only, optional leading `-`, no leading zeros (except `0`).
fn json_infer_int(value: &str) -> Option<Scalar> {
    let digits = value.strip_prefix('-').unwrap_or(value);
    if digits.is_empty() {
        return None;
    }
    // No leading zeros unless the entire number is "0".
    if digits.len() > 1 && digits.starts_with('0') {
        return None;
    }
    if !digits.chars().all(|c| c.is_ascii_digit()) {
        return None;
    }
    value.parse::<i64>().ok().map(Scalar::Int)
}

// Core int: decimal, octal (`0o…`), or hex (`0x…`), optional leading `+`/`-`.
// Leading zeros in decimal (e.g. `007`) are not valid.
fn core_infer_int(value: &str) -> Option<Scalar> {
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
        // Decimal — no leading zeros (unless the value is exactly "0").
        if rest.len() > 1 && rest.starts_with('0') {
            return None;
        }
        if !rest.chars().all(|c| c.is_ascii_digit()) {
            return None;
        }
        rest.parse::<i64>().ok()?
    };
    Some(Scalar::Int(if neg { -magnitude } else { magnitude }))
}

// JSON float: decimal with `.` or `e`/`E`, plus `.inf`/`-.inf`/`.nan`.
fn json_infer_float(value: &str) -> Option<Scalar> {
    match value {
        ".inf" => return Some(Scalar::Float(f64::INFINITY)),
        "-.inf" => return Some(Scalar::Float(f64::NEG_INFINITY)),
        ".nan" => return Some(Scalar::Float(f64::NAN)),
        _ => {}
    }
    let signed = value.strip_prefix('-').unwrap_or(value);
    if signed.contains('.') || signed.contains('e') || signed.contains('E') {
        return value.parse::<f64>().ok().map(Scalar::Float);
    }
    None
}

// Core float: extends JSON with case variants of .inf/.nan and `+` prefix.
fn core_infer_float(value: &str) -> Option<Scalar> {
    match value {
        ".inf" | ".Inf" | ".INF" => return Some(Scalar::Float(f64::INFINITY)),
        "-.inf" | "-.Inf" | "-.INF" => return Some(Scalar::Float(f64::NEG_INFINITY)),
        ".nan" | ".NaN" | ".NAN" => return Some(Scalar::Float(f64::NAN)),
        _ => {}
    }
    let stripped = value.strip_prefix('+').unwrap_or(value);
    let signed = stripped.strip_prefix('-').unwrap_or(stripped);
    if signed.contains('.') || signed.contains('e') || signed.contains('E') {
        return value
            .trim_start_matches('+')
            .parse::<f64>()
            .ok()
            .map(Scalar::Float);
    }
    None
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
#[allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::doc_markdown,
    clippy::float_cmp
)]
mod tests {
    use super::*;
    use crate::event::{Chomp, ScalarStyle};

    // -----------------------------------------------------------------------
    // Helpers
    // -----------------------------------------------------------------------

    fn core() -> CoreSchema {
        CoreSchema
    }
    fn json() -> JsonSchema {
        JsonSchema
    }
    fn failsafe() -> FailsafeSchema {
        FailsafeSchema
    }

    fn assert_float_eq(actual: &Scalar, expected: f64) {
        match actual {
            Scalar::Float(f) => {
                assert!(
                    (f - expected).abs() < 1e-10,
                    "expected Float({expected}), got Float({f})"
                );
            }
            other @ (Scalar::Null | Scalar::Bool(_) | Scalar::Int(_) | Scalar::String(_)) => {
                panic!("expected Scalar::Float, got {other:?}")
            }
        }
    }

    fn assert_float_inf_pos(actual: &Scalar) {
        match actual {
            Scalar::Float(f) => assert!(f.is_infinite() && f.is_sign_positive()),
            other @ (Scalar::Null | Scalar::Bool(_) | Scalar::Int(_) | Scalar::String(_)) => {
                panic!("expected positive infinity, got {other:?}")
            }
        }
    }

    fn assert_float_inf_neg(actual: &Scalar) {
        match actual {
            Scalar::Float(f) => assert!(f.is_infinite() && f.is_sign_negative()),
            other @ (Scalar::Null | Scalar::Bool(_) | Scalar::Int(_) | Scalar::String(_)) => {
                panic!("expected negative infinity, got {other:?}")
            }
        }
    }

    fn assert_float_nan(actual: &Scalar) {
        match actual {
            Scalar::Float(f) => assert!(f.is_nan(), "expected NaN, got {f}"),
            other @ (Scalar::Null | Scalar::Bool(_) | Scalar::Int(_) | Scalar::String(_)) => {
                panic!("expected NaN float, got {other:?}")
            }
        }
    }

    // -----------------------------------------------------------------------
    // Group 1: Scalar type — construction and equality
    // -----------------------------------------------------------------------

    /// Test 1 — spike: CoreSchema resolves "null" to Scalar::Null
    #[test]
    fn scalar_null_equals_null() {
        assert_eq!(Scalar::Null, Scalar::Null);
        // Spike: confirm the whole pipeline works.
        assert_eq!(
            core().resolve_scalar("null", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 2 — Bool(true) equals Bool(true)
    #[test]
    fn scalar_bool_true_equals_bool_true() {
        assert_eq!(Scalar::Bool(true), Scalar::Bool(true));
    }

    /// Test 3 — Bool(true) does not equal Bool(false)
    #[test]
    fn scalar_bool_true_does_not_equal_false() {
        assert_ne!(Scalar::Bool(true), Scalar::Bool(false));
    }

    /// Test 4 — Int equality
    #[test]
    fn scalar_int_equality() {
        assert_eq!(Scalar::Int(42), Scalar::Int(42));
    }

    /// Test 5 — String equality
    #[test]
    fn scalar_string_equality() {
        assert_eq!(
            Scalar::String("hello".to_owned()),
            Scalar::String("hello".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 2: FailsafeSchema — all scalars are strings
    // -----------------------------------------------------------------------

    /// Test 6 — failsafe plain "null" is String
    #[test]
    fn failsafe_plain_null_is_string() {
        assert_eq!(
            failsafe().resolve_scalar("null", None, ScalarStyle::Plain),
            Scalar::String("null".to_owned())
        );
    }

    /// Test 7 — failsafe plain "true" is String
    #[test]
    fn failsafe_plain_true_is_string() {
        assert_eq!(
            failsafe().resolve_scalar("true", None, ScalarStyle::Plain),
            Scalar::String("true".to_owned())
        );
    }

    /// Test 8 — failsafe plain "42" is String
    #[test]
    fn failsafe_plain_integer_is_string() {
        assert_eq!(
            failsafe().resolve_scalar("42", None, ScalarStyle::Plain),
            Scalar::String("42".to_owned())
        );
    }

    /// Test 9 — failsafe single-quoted "hello" is String
    #[test]
    fn failsafe_quoted_value_is_string() {
        assert_eq!(
            failsafe().resolve_scalar("hello", None, ScalarStyle::SingleQuoted),
            Scalar::String("hello".to_owned())
        );
    }

    /// Test 10 — failsafe ignores explicit int tag; still String
    #[test]
    fn failsafe_explicit_tag_ignored_still_string() {
        assert_eq!(
            failsafe().resolve_scalar("42", Some("tag:yaml.org,2002:int"), ScalarStyle::Plain),
            Scalar::String("42".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 3: CoreSchema — null resolution
    // -----------------------------------------------------------------------

    /// Test 11 — plain "null" (lowercase) is Null
    #[test]
    fn core_plain_null_lowercase_is_null() {
        assert_eq!(
            core().resolve_scalar("null", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 12 — plain "Null" (titlecase) is Null
    #[test]
    fn core_plain_null_titlecase_is_null() {
        assert_eq!(
            core().resolve_scalar("Null", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 13 — plain "NULL" (uppercase) is Null
    #[test]
    fn core_plain_null_uppercase_is_null() {
        assert_eq!(
            core().resolve_scalar("NULL", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 14 — plain "~" is Null
    #[test]
    fn core_plain_tilde_is_null() {
        assert_eq!(
            core().resolve_scalar("~", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 15 — plain empty scalar is Null
    #[test]
    fn core_empty_plain_scalar_is_null() {
        assert_eq!(
            core().resolve_scalar("", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 16 — single-quoted "null" is String (quoted bypasses inference)
    #[test]
    fn core_quoted_null_is_string() {
        assert_eq!(
            core().resolve_scalar("null", None, ScalarStyle::SingleQuoted),
            Scalar::String("null".to_owned())
        );
    }

    /// Test 17 — double-quoted empty is String, not Null
    #[test]
    fn core_quoted_empty_is_string() {
        assert_eq!(
            core().resolve_scalar("", None, ScalarStyle::DoubleQuoted),
            Scalar::String(String::new())
        );
    }

    // -----------------------------------------------------------------------
    // Group 4: CoreSchema — bool resolution
    // -----------------------------------------------------------------------

    /// Test 18 — plain "true" (lowercase) is Bool(true)
    #[test]
    fn core_plain_true_lowercase_is_bool_true() {
        assert_eq!(
            core().resolve_scalar("true", None, ScalarStyle::Plain),
            Scalar::Bool(true)
        );
    }

    /// Test 19 — plain "false" (lowercase) is Bool(false)
    #[test]
    fn core_plain_false_lowercase_is_bool_false() {
        assert_eq!(
            core().resolve_scalar("false", None, ScalarStyle::Plain),
            Scalar::Bool(false)
        );
    }

    /// Test 20 — plain "True" (titlecase) is Bool(true)
    #[test]
    fn core_plain_true_titlecase_is_bool_true() {
        assert_eq!(
            core().resolve_scalar("True", None, ScalarStyle::Plain),
            Scalar::Bool(true)
        );
    }

    /// Test 21 — plain "False" (titlecase) is Bool(false)
    #[test]
    fn core_plain_false_titlecase_is_bool_false() {
        assert_eq!(
            core().resolve_scalar("False", None, ScalarStyle::Plain),
            Scalar::Bool(false)
        );
    }

    /// Test 22 — plain "TRUE" (uppercase) is Bool(true)
    #[test]
    fn core_plain_true_uppercase_is_bool_true() {
        assert_eq!(
            core().resolve_scalar("TRUE", None, ScalarStyle::Plain),
            Scalar::Bool(true)
        );
    }

    /// Test 23 — plain "FALSE" (uppercase) is Bool(false)
    #[test]
    fn core_plain_false_uppercase_is_bool_false() {
        assert_eq!(
            core().resolve_scalar("FALSE", None, ScalarStyle::Plain),
            Scalar::Bool(false)
        );
    }

    /// Test 24 — single-quoted "true" is String
    #[test]
    fn core_quoted_true_is_string() {
        assert_eq!(
            core().resolve_scalar("true", None, ScalarStyle::SingleQuoted),
            Scalar::String("true".to_owned())
        );
    }

    /// Test 25 — double-quoted "false" is String
    #[test]
    fn core_quoted_false_is_string() {
        assert_eq!(
            core().resolve_scalar("false", None, ScalarStyle::DoubleQuoted),
            Scalar::String("false".to_owned())
        );
    }

    /// Test 26 — plain "yes" is String (YAML 1.2 Core does not recognize yes/no)
    #[test]
    fn core_plain_yes_is_string() {
        assert_eq!(
            core().resolve_scalar("yes", None, ScalarStyle::Plain),
            Scalar::String("yes".to_owned())
        );
    }

    /// Test 27 — plain "on" is String (YAML 1.2 Core does not recognize on/off)
    #[test]
    fn core_plain_on_is_string() {
        assert_eq!(
            core().resolve_scalar("on", None, ScalarStyle::Plain),
            Scalar::String("on".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 5: CoreSchema — integer resolution
    // -----------------------------------------------------------------------

    /// Test 28 — plain "0" is Int(0)
    #[test]
    fn core_plain_decimal_zero_is_int() {
        assert_eq!(
            core().resolve_scalar("0", None, ScalarStyle::Plain),
            Scalar::Int(0)
        );
    }

    /// Test 29 — plain "42" is Int(42)
    #[test]
    fn core_plain_positive_decimal_is_int() {
        assert_eq!(
            core().resolve_scalar("42", None, ScalarStyle::Plain),
            Scalar::Int(42)
        );
    }

    /// Test 30 — plain "-17" is Int(-17)
    #[test]
    fn core_plain_negative_decimal_is_int() {
        assert_eq!(
            core().resolve_scalar("-17", None, ScalarStyle::Plain),
            Scalar::Int(-17)
        );
    }

    /// Test 31 — plain "+5" is Int(5)
    #[test]
    fn core_plain_explicit_plus_decimal_is_int() {
        assert_eq!(
            core().resolve_scalar("+5", None, ScalarStyle::Plain),
            Scalar::Int(5)
        );
    }

    /// Test 32 — plain "0o777" is Int(511)
    #[test]
    fn core_plain_octal_is_int() {
        assert_eq!(
            core().resolve_scalar("0o777", None, ScalarStyle::Plain),
            Scalar::Int(0o777)
        );
    }

    /// Test 33 — plain "0o0" is Int(0)
    #[test]
    fn core_plain_octal_zero_is_int() {
        assert_eq!(
            core().resolve_scalar("0o0", None, ScalarStyle::Plain),
            Scalar::Int(0)
        );
    }

    /// Test 34 — plain "-0o7" is Int(-7)
    #[test]
    fn core_plain_negative_octal_is_int() {
        assert_eq!(
            core().resolve_scalar("-0o7", None, ScalarStyle::Plain),
            Scalar::Int(-7)
        );
    }

    /// Test 35 — plain "0xFF" is Int(255)
    #[test]
    fn core_plain_hex_is_int() {
        assert_eq!(
            core().resolve_scalar("0xFF", None, ScalarStyle::Plain),
            Scalar::Int(255)
        );
    }

    /// Test 36 — plain "0xff" is Int(255)
    #[test]
    fn core_plain_hex_lowercase_is_int() {
        assert_eq!(
            core().resolve_scalar("0xff", None, ScalarStyle::Plain),
            Scalar::Int(255)
        );
    }

    /// Test 37 — plain "-0x10" is Int(-16)
    #[test]
    fn core_plain_negative_hex_is_int() {
        assert_eq!(
            core().resolve_scalar("-0x10", None, ScalarStyle::Plain),
            Scalar::Int(-16)
        );
    }

    /// Test 38 — plain "007" is String (leading zeros are not octal in YAML 1.2)
    #[test]
    fn core_plain_leading_zero_decimal_is_string() {
        assert_eq!(
            core().resolve_scalar("007", None, ScalarStyle::Plain),
            Scalar::String("007".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 6: CoreSchema — float resolution
    // -----------------------------------------------------------------------

    /// Test 39 — plain "1.25" is Float(1.25)
    #[test]
    fn core_plain_decimal_float_is_float() {
        let result = core().resolve_scalar("1.25", None, ScalarStyle::Plain);
        assert_float_eq(&result, 1.25_f64);
    }

    /// Test 40 — plain "-1.5" is Float(-1.5)
    #[test]
    fn core_plain_negative_float_is_float() {
        let result = core().resolve_scalar("-1.5", None, ScalarStyle::Plain);
        assert_float_eq(&result, -1.5_f64);
    }

    /// Test 41 — plain "+0.5" is Float(0.5)
    #[test]
    fn core_plain_positive_float_is_float() {
        let result = core().resolve_scalar("+0.5", None, ScalarStyle::Plain);
        assert_float_eq(&result, 0.5_f64);
    }

    /// Test 42 — plain "1.0e10" is Float(1.0e10)
    #[test]
    fn core_plain_float_scientific_lowercase_e_is_float() {
        let result = core().resolve_scalar("1.0e10", None, ScalarStyle::Plain);
        assert_float_eq(&result, 1.0e10_f64);
    }

    /// Test 43 — plain "2.5E3" is Float(2500.0)
    #[test]
    fn core_plain_float_scientific_uppercase_e_is_float() {
        let result = core().resolve_scalar("2.5E3", None, ScalarStyle::Plain);
        assert_float_eq(&result, 2500.0_f64);
    }

    /// Test 44 — plain ".inf" is positive infinity
    #[test]
    fn core_plain_dot_inf_lowercase_is_positive_infinity() {
        let result = core().resolve_scalar(".inf", None, ScalarStyle::Plain);
        assert_float_inf_pos(&result);
    }

    /// Test 45 — plain ".Inf" is positive infinity
    #[test]
    fn core_plain_dot_inf_titlecase_is_positive_infinity() {
        let result = core().resolve_scalar(".Inf", None, ScalarStyle::Plain);
        assert_float_inf_pos(&result);
    }

    /// Test 46 — plain ".INF" is positive infinity
    #[test]
    fn core_plain_dot_inf_uppercase_is_positive_infinity() {
        let result = core().resolve_scalar(".INF", None, ScalarStyle::Plain);
        assert_float_inf_pos(&result);
    }

    /// Test 47 — plain "-.inf" is negative infinity
    #[test]
    fn core_plain_negative_dot_inf_lowercase_is_negative_infinity() {
        let result = core().resolve_scalar("-.inf", None, ScalarStyle::Plain);
        assert_float_inf_neg(&result);
    }

    /// Test 48 — plain "-.Inf" is negative infinity
    #[test]
    fn core_plain_negative_dot_inf_titlecase_is_negative_infinity() {
        let result = core().resolve_scalar("-.Inf", None, ScalarStyle::Plain);
        assert_float_inf_neg(&result);
    }

    /// Test 49 — plain "-.INF" is negative infinity
    #[test]
    fn core_plain_negative_dot_inf_uppercase_is_negative_infinity() {
        let result = core().resolve_scalar("-.INF", None, ScalarStyle::Plain);
        assert_float_inf_neg(&result);
    }

    /// Test 50 — plain ".nan" is NaN
    #[test]
    fn core_plain_dot_nan_lowercase_is_nan() {
        let result = core().resolve_scalar(".nan", None, ScalarStyle::Plain);
        assert_float_nan(&result);
    }

    /// Test 51 — plain ".NaN" is NaN
    #[test]
    fn core_plain_dot_nan_titlecase_is_nan() {
        let result = core().resolve_scalar(".NaN", None, ScalarStyle::Plain);
        assert_float_nan(&result);
    }

    /// Test 52 — plain ".NAN" is NaN
    #[test]
    fn core_plain_dot_nan_uppercase_is_nan() {
        let result = core().resolve_scalar(".NAN", None, ScalarStyle::Plain);
        assert_float_nan(&result);
    }

    // -----------------------------------------------------------------------
    // Group 7: CoreSchema — string fallback
    // -----------------------------------------------------------------------

    /// Test 53 — plain "hello" is String
    #[test]
    fn core_plain_arbitrary_word_is_string() {
        assert_eq!(
            core().resolve_scalar("hello", None, ScalarStyle::Plain),
            Scalar::String("hello".to_owned())
        );
    }

    /// Test 54 — plain "42abc" is String
    #[test]
    fn core_plain_integer_looking_with_letters_is_string() {
        assert_eq!(
            core().resolve_scalar("42abc", None, ScalarStyle::Plain),
            Scalar::String("42abc".to_owned())
        );
    }

    /// Test 55 — plain "0o" (bare octal prefix, no digits) is String
    #[test]
    fn core_plain_partial_octal_prefix_is_string() {
        assert_eq!(
            core().resolve_scalar("0o", None, ScalarStyle::Plain),
            Scalar::String("0o".to_owned())
        );
    }

    /// Test 56 — plain "0x" (bare hex prefix, no digits) is String
    #[test]
    fn core_plain_partial_hex_prefix_is_string() {
        assert_eq!(
            core().resolve_scalar("0x", None, ScalarStyle::Plain),
            Scalar::String("0x".to_owned())
        );
    }

    /// Test 57 — literal block style is String (bypasses inference)
    #[test]
    fn core_literal_block_scalar_is_string() {
        assert_eq!(
            core().resolve_scalar("hello\n", None, ScalarStyle::Literal(Chomp::Clip)),
            Scalar::String("hello\n".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 8: Explicit tag override
    // -----------------------------------------------------------------------

    /// Test 58 — !!str tag on integer-looking plain forces String
    #[test]
    fn core_str_tag_on_integer_looking_plain_is_string() {
        assert_eq!(
            core().resolve_scalar("123", Some("tag:yaml.org,2002:str"), ScalarStyle::Plain),
            Scalar::String("123".to_owned())
        );
    }

    /// Test 59 — !!str tag on "null"-looking plain forces String
    #[test]
    fn core_str_tag_on_null_looking_plain_is_string() {
        assert_eq!(
            core().resolve_scalar("null", Some("tag:yaml.org,2002:str"), ScalarStyle::Plain),
            Scalar::String("null".to_owned())
        );
    }

    /// Test 60 — !!str tag on "true"-looking plain forces String
    #[test]
    fn core_str_tag_on_bool_looking_plain_is_string() {
        assert_eq!(
            core().resolve_scalar("true", Some("tag:yaml.org,2002:str"), ScalarStyle::Plain),
            Scalar::String("true".to_owned())
        );
    }

    /// Test 61 — !!int tag on decimal plain forces Int
    #[test]
    fn core_int_tag_on_decimal_is_int() {
        assert_eq!(
            core().resolve_scalar("42", Some("tag:yaml.org,2002:int"), ScalarStyle::Plain),
            Scalar::Int(42)
        );
    }

    /// Test 62 — !!float tag on decimal plain forces Float
    #[test]
    fn core_float_tag_on_decimal_is_float() {
        let result =
            core().resolve_scalar("1.25", Some("tag:yaml.org,2002:float"), ScalarStyle::Plain);
        assert_float_eq(&result, 1.25_f64);
    }

    /// Test 63 — !!null tag forces Null regardless of value
    #[test]
    fn core_null_tag_on_string_value_is_null() {
        assert_eq!(
            core().resolve_scalar(
                "anything",
                Some("tag:yaml.org,2002:null"),
                ScalarStyle::Plain
            ),
            Scalar::Null
        );
    }

    /// Test 64 — !!bool tag on "true" is Bool(true)
    #[test]
    fn core_bool_tag_on_true_is_bool() {
        assert_eq!(
            core().resolve_scalar("true", Some("tag:yaml.org,2002:bool"), ScalarStyle::Plain),
            Scalar::Bool(true)
        );
    }

    /// Test 65 — !!bool tag on "false" is Bool(false)
    #[test]
    fn core_bool_tag_on_false_is_bool() {
        assert_eq!(
            core().resolve_scalar("false", Some("tag:yaml.org,2002:bool"), ScalarStyle::Plain),
            Scalar::Bool(false)
        );
    }

    /// Test 66 — !!str shorthand (expanded to tag:yaml.org,2002:str) forces String
    ///
    /// Tag shorthand expansion (!!str → tag:yaml.org,2002:str) is the caller's
    /// responsibility; resolve_scalar receives the already-expanded form.
    #[test]
    fn core_bang_str_shorthand_tag_is_string() {
        assert_eq!(
            core().resolve_scalar("123", Some("tag:yaml.org,2002:str"), ScalarStyle::Plain),
            Scalar::String("123".to_owned())
        );
    }

    /// Test 67 — verbatim !<tag:yaml.org,2002:str> (expanded form) forces String
    #[test]
    fn core_verbatim_str_tag_is_string() {
        assert_eq!(
            core().resolve_scalar("42", Some("tag:yaml.org,2002:str"), ScalarStyle::Plain),
            Scalar::String("42".to_owned())
        );
    }

    /// Test 68 — unknown tag falls back to String (no panic)
    #[test]
    fn core_unknown_tag_falls_back_to_string() {
        assert_eq!(
            core().resolve_scalar("hello", Some("tag:example.com:custom"), ScalarStyle::Plain),
            Scalar::String("hello".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 9: Non-specific tags `!` and `?`
    // -----------------------------------------------------------------------

    /// Test 69 — `!` tag forces String
    #[test]
    fn core_bang_tag_forces_string() {
        assert_eq!(
            core().resolve_scalar("null", Some("!"), ScalarStyle::Plain),
            Scalar::String("null".to_owned())
        );
    }

    /// Test 70 — `?` tag uses schema inference (null)
    #[test]
    fn core_question_tag_uses_schema_inference() {
        assert_eq!(
            core().resolve_scalar("null", Some("?"), ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 71 — `?` tag on plain "42" uses inference (Int)
    #[test]
    fn core_question_tag_on_plain_int_uses_inference() {
        assert_eq!(
            core().resolve_scalar("42", Some("?"), ScalarStyle::Plain),
            Scalar::Int(42)
        );
    }

    /// Test 72 — `?` tag on single-quoted "42" is String (quoted bypasses inference)
    #[test]
    fn core_question_tag_on_quoted_int_is_string() {
        assert_eq!(
            core().resolve_scalar("42", Some("?"), ScalarStyle::SingleQuoted),
            Scalar::String("42".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 10: JsonSchema
    // -----------------------------------------------------------------------

    /// Test 73 — json: only lowercase "null" is Null
    #[test]
    fn json_plain_null_only_lowercase_is_null() {
        assert_eq!(
            json().resolve_scalar("null", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 74 — json: "Null" (titlecase) is String
    #[test]
    fn json_plain_null_titlecase_is_string() {
        assert_eq!(
            json().resolve_scalar("Null", None, ScalarStyle::Plain),
            Scalar::String("Null".to_owned())
        );
    }

    /// Test 75 — json: "~" is String (not null in JSON schema)
    #[test]
    fn json_plain_tilde_is_string() {
        assert_eq!(
            json().resolve_scalar("~", None, ScalarStyle::Plain),
            Scalar::String("~".to_owned())
        );
    }

    /// Test 76 — json: empty plain is String (not null in JSON schema)
    #[test]
    fn json_plain_empty_is_string() {
        assert_eq!(
            json().resolve_scalar("", None, ScalarStyle::Plain),
            Scalar::String(String::new())
        );
    }

    /// Test 77 — json: "true" is Bool(true)
    #[test]
    fn json_plain_true_lowercase_is_bool_true() {
        assert_eq!(
            json().resolve_scalar("true", None, ScalarStyle::Plain),
            Scalar::Bool(true)
        );
    }

    /// Test 78 — json: "false" is Bool(false)
    #[test]
    fn json_plain_false_lowercase_is_bool_false() {
        assert_eq!(
            json().resolve_scalar("false", None, ScalarStyle::Plain),
            Scalar::Bool(false)
        );
    }

    /// Test 79 — json: "True" (titlecase) is String
    #[test]
    fn json_plain_true_titlecase_is_string() {
        assert_eq!(
            json().resolve_scalar("True", None, ScalarStyle::Plain),
            Scalar::String("True".to_owned())
        );
    }

    /// Test 80 — json: "42" is Int(42)
    #[test]
    fn json_plain_decimal_int_is_int() {
        assert_eq!(
            json().resolve_scalar("42", None, ScalarStyle::Plain),
            Scalar::Int(42)
        );
    }

    /// Test 81 — json: "-5" is Int(-5)
    #[test]
    fn json_plain_negative_decimal_int_is_int() {
        assert_eq!(
            json().resolve_scalar("-5", None, ScalarStyle::Plain),
            Scalar::Int(-5)
        );
    }

    /// Test 82 — json: "0o77" is String (no octal in JSON schema)
    #[test]
    fn json_plain_octal_is_string() {
        assert_eq!(
            json().resolve_scalar("0o77", None, ScalarStyle::Plain),
            Scalar::String("0o77".to_owned())
        );
    }

    /// Test 83 — json: "0xFF" is String (no hex in JSON schema)
    #[test]
    fn json_plain_hex_is_string() {
        assert_eq!(
            json().resolve_scalar("0xFF", None, ScalarStyle::Plain),
            Scalar::String("0xFF".to_owned())
        );
    }

    /// Test 84 — json: "1.5" is Float(1.5)
    #[test]
    fn json_plain_decimal_float_is_float() {
        let result = json().resolve_scalar("1.5", None, ScalarStyle::Plain);
        assert_float_eq(&result, 1.5_f64);
    }

    /// Test 85 — json: ".inf" is positive infinity
    #[test]
    fn json_plain_dot_inf_is_float_infinity() {
        let result = json().resolve_scalar(".inf", None, ScalarStyle::Plain);
        assert_float_inf_pos(&result);
    }

    /// Test 86 — json: ".nan" is NaN
    #[test]
    fn json_plain_dot_nan_is_float_nan() {
        let result = json().resolve_scalar(".nan", None, ScalarStyle::Plain);
        assert_float_nan(&result);
    }

    /// Test 87 — json: double-quoted "null" is String
    #[test]
    fn json_quoted_null_is_string() {
        assert_eq!(
            json().resolve_scalar("null", None, ScalarStyle::DoubleQuoted),
            Scalar::String("null".to_owned())
        );
    }

    // -----------------------------------------------------------------------
    // Group 11: Schema trait — pluggable custom schema
    // -----------------------------------------------------------------------

    /// Test 88 — Schema is object-safe and callable via &dyn Schema
    #[test]
    fn custom_schema_via_trait_object_is_callable() {
        struct AlwaysNull;
        impl Schema for AlwaysNull {
            fn resolve_scalar(
                &self,
                _value: &str,
                _tag: Option<&str>,
                _style: ScalarStyle,
            ) -> Scalar {
                Scalar::Null
            }
            fn resolve_tag(&self, tag: &str) -> ResolvedTag {
                ResolvedTag::Unknown(tag.to_owned())
            }
        }
        let schema: &dyn Schema = &AlwaysNull;
        assert_eq!(
            schema.resolve_scalar("hello", None, ScalarStyle::Plain),
            Scalar::Null
        );
    }

    /// Test 89 — custom schema overrides core behavior
    #[test]
    fn custom_schema_overrides_core_behavior() {
        struct StringOnly;
        impl Schema for StringOnly {
            fn resolve_scalar(
                &self,
                value: &str,
                _tag: Option<&str>,
                _style: ScalarStyle,
            ) -> Scalar {
                Scalar::String(value.to_owned())
            }
            fn resolve_tag(&self, tag: &str) -> ResolvedTag {
                ResolvedTag::Unknown(tag.to_owned())
            }
        }
        assert_eq!(
            StringOnly.resolve_scalar("42", None, ScalarStyle::Plain),
            Scalar::String("42".to_owned())
        );
    }

    /// Test 90 — resolve_tag on CoreSchema returns known tag for str
    #[test]
    fn resolve_tag_on_core_schema_returns_known_tags() {
        assert_eq!(
            core().resolve_tag("tag:yaml.org,2002:str"),
            ResolvedTag::Str
        );
    }

    /// Test 91 — resolve_tag on CoreSchema returns Unknown for foreign tag
    #[test]
    fn resolve_tag_on_core_schema_returns_unknown_for_foreign_tag() {
        assert!(matches!(
            core().resolve_tag("tag:example.com:custom"),
            ResolvedTag::Unknown(_)
        ));
    }

    // -----------------------------------------------------------------------
    // Group 12: Integration — schema module accessible from crate root
    // -----------------------------------------------------------------------

    /// Test 92 — schema module is accessible from crate
    #[test]
    fn schema_module_is_accessible_from_crate() {
        let result = crate::schema::CoreSchema.resolve_scalar("null", None, ScalarStyle::Plain);
        assert_eq!(result, Scalar::Null);
    }

    /// Test 93 — Scalar is accessible from crate
    #[test]
    fn schema_scalar_is_accessible_from_crate() {
        let s = crate::schema::Scalar::Null;
        assert_ne!(s, crate::schema::Scalar::Bool(false));
    }

    /// Test 94 — CoreSchema resolves "42" to Int(42) (default schema behavior)
    #[test]
    fn core_schema_is_default_schema() {
        // CoreSchema is a unit struct; Default is derived.
        assert_eq!(
            CoreSchema.resolve_scalar("42", None, ScalarStyle::Plain),
            Scalar::Int(42)
        );
    }

    /// Test 95 — FailsafeSchema is accessible from crate
    #[test]
    fn failsafe_schema_is_accessible_from_crate() {
        assert_eq!(
            crate::schema::FailsafeSchema.resolve_scalar("null", None, ScalarStyle::Plain),
            Scalar::String("null".to_owned())
        );
    }

    /// Test 96 — JsonSchema is accessible from crate
    #[test]
    fn json_schema_is_accessible_from_crate() {
        assert_eq!(
            crate::schema::JsonSchema.resolve_scalar("true", None, ScalarStyle::Plain),
            Scalar::Bool(true)
        );
    }
}
