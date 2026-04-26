// SPDX-License-Identifier: MIT

//! YAML 1.2.2 §10 schema tag resolution.
//!
//! Three schemas are provided, in increasing generality:
//!
//! - [`Schema::Failsafe`] — all scalars resolve to `!!str`, all sequences to
//!   `!!seq`, all mappings to `!!map`.
//! - [`Schema::Json`] — narrow pattern set; unmatched plain scalars are an
//!   error ([`UnresolvedScalar`]).
//! - [`Schema::Core`] — superset of JSON; unmatched plain scalars fall back to
//!   `!!str`.
//!
//! Use [`resolve_scalar`] and [`resolve_collection`] to apply a schema to a
//! node.  When the node already carries an explicit source tag, both functions
//! return `None` / `Ok(None)` — the caller's tag takes precedence.

use crate::event::ScalarStyle;

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// YAML 1.2.2 §10 recommended schema selection.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Schema {
    /// Failsafe schema (§10.1): scalars → `str`, sequences → `seq`,
    /// mappings → `map`.
    Failsafe,
    /// JSON schema (§10.2): narrow pattern set; unmatched plain scalars
    /// produce [`UnresolvedScalar`].
    Json,
    /// Core schema (§10.3): superset of JSON; unmatched plain scalars fall
    /// back to `str`.
    Core,
}

/// The resolved YAML tag for a node.
///
/// Each variant carries the URI constant for that tag family.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
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
}

impl ResolvedTag {
    /// Returns the `tag:yaml.org,2002:*` URI for this tag.
    #[must_use]
    pub const fn as_str(self) -> &'static str {
        match self {
            Self::Str => "tag:yaml.org,2002:str",
            Self::Int => "tag:yaml.org,2002:int",
            Self::Float => "tag:yaml.org,2002:float",
            Self::Bool => "tag:yaml.org,2002:bool",
            Self::Null => "tag:yaml.org,2002:null",
            Self::Seq => "tag:yaml.org,2002:seq",
            Self::Map => "tag:yaml.org,2002:map",
        }
    }
}

/// Error returned by [`resolve_scalar`] when the JSON schema cannot match a
/// plain scalar value.
///
/// The JSON schema has no fallback — every untagged plain scalar must match one
/// of its patterns (null, bool, int, float).  If none match, the scalar is
/// unresolvable under JSON schema rules.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unresolved scalar: no JSON schema pattern matched the plain scalar value")]
pub struct UnresolvedScalar;

/// Collection kind, used as a parameter to [`resolve_collection`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CollectionKind {
    /// A YAML sequence (`!!seq`).
    Sequence,
    /// A YAML mapping (`!!map`).
    Mapping,
}

// ---------------------------------------------------------------------------
// Resolution functions
// ---------------------------------------------------------------------------

/// Resolve the tag for a scalar node under the given schema.
///
/// # Return value
///
/// - `Ok(None)` — `source_tag` is `Some`; the existing explicit tag wins, no
///   schema resolution applied.
/// - `Ok(Some(tag))` — resolution succeeded; `tag` is the resolved YAML tag.
///
/// # Errors
///
/// Returns [`Err(UnresolvedScalar)`](UnresolvedScalar) only with
/// [`Schema::Json`] when the scalar style is [`ScalarStyle::Plain`] and no
/// JSON pattern matched.
///
/// # Style semantics
///
/// Only [`ScalarStyle::Plain`] scalars participate in pattern matching.  All
/// other styles (single-quoted, double-quoted, literal block, folded block)
/// resolve unconditionally to `!!str` — the content of a quoted or block scalar
/// is always a string regardless of what the characters spell.
pub fn resolve_scalar(
    schema: Schema,
    style: ScalarStyle,
    value: &str,
    source_tag: Option<&str>,
) -> Result<Option<ResolvedTag>, UnresolvedScalar> {
    // Explicit source tag takes priority over schema resolution.
    if source_tag.is_some() {
        return Ok(None);
    }

    match schema {
        Schema::Failsafe => Ok(Some(ResolvedTag::Str)),

        Schema::Core => {
            let tag = match style {
                ScalarStyle::Plain => resolve_core_plain(value),
                // All non-plain styles are unconditionally !!str.
                ScalarStyle::SingleQuoted
                | ScalarStyle::DoubleQuoted
                | ScalarStyle::Literal(_)
                | ScalarStyle::Folded(_) => ResolvedTag::Str,
            };
            Ok(Some(tag))
        }

        Schema::Json => {
            let tag = match style {
                ScalarStyle::Plain => resolve_json_plain(value)?,
                // Non-plain styles are !!str in JSON schema too.
                ScalarStyle::SingleQuoted
                | ScalarStyle::DoubleQuoted
                | ScalarStyle::Literal(_)
                | ScalarStyle::Folded(_) => ResolvedTag::Str,
            };
            Ok(Some(tag))
        }
    }
}

/// Resolve the tag for a collection node under the given schema.
///
/// # Return value
///
/// - `None` — `source_tag` is `Some`; the existing explicit tag wins.
/// - `Some(tag)` — resolved tag (`Seq` or `Map`) according to `kind`.
///
/// All three schemas resolve sequences to `!!seq` and mappings to `!!map`.
#[must_use]
pub const fn resolve_collection(
    schema: Schema,
    kind: CollectionKind,
    source_tag: Option<&str>,
) -> Option<ResolvedTag> {
    // Explicit source tag wins.
    if source_tag.is_some() {
        return None;
    }
    // All three schemas map sequences → !!seq and mappings → !!map.
    let _ = schema;
    Some(match kind {
        CollectionKind::Sequence => ResolvedTag::Seq,
        CollectionKind::Mapping => ResolvedTag::Map,
    })
}

// ---------------------------------------------------------------------------
// Core schema plain-scalar dispatch (§10.3)
// ---------------------------------------------------------------------------

/// Resolve a plain scalar under the Core schema.
///
/// Dispatches on the first byte to prune the common-case `Str` outcome before
/// any pattern matcher runs. Each branch covers exactly the prefix set of the
/// matcher(s) it invokes — bytes outside the enumerated set can only be `Str`.
fn resolve_core_plain(value: &str) -> ResolvedTag {
    match value.as_bytes().first().copied() {
        // Empty string or "~" → null (the only two direct-return null forms).
        None | Some(b'~') => ResolvedTag::Null,
        // "null" | "Null" | "NULL" start with 'n'/'N'; only null uses these.
        Some(b'n' | b'N') => {
            if is_core_null(value) {
                ResolvedTag::Null
            } else {
                ResolvedTag::Str
            }
        }
        // "true"/"True"/"TRUE"/"false"/"False"/"FALSE".
        Some(b't' | b'T' | b'f' | b'F') => {
            if is_core_bool(value) {
                ResolvedTag::Bool
            } else {
                ResolvedTag::Str
            }
        }
        // Decimal/octal/hex integers and decimal floats with a leading digit or sign.
        Some(b'-' | b'+' | b'0'..=b'9') => {
            if is_core_int(value) {
                ResolvedTag::Int
            } else if is_core_float(value) {
                ResolvedTag::Float
            } else {
                ResolvedTag::Str
            }
        }
        // ".inf"/".Inf"/".INF"/".nan"/".NaN"/".NAN" and leading-dot decimal floats.
        Some(b'.') => {
            if is_core_float(value) {
                ResolvedTag::Float
            } else {
                ResolvedTag::Str
            }
        }
        // Any other first byte cannot match null/bool/int/float — return Str directly.
        Some(_) => ResolvedTag::Str,
    }
}

// ---------------------------------------------------------------------------
// JSON schema plain-scalar dispatch (§10.2)
// ---------------------------------------------------------------------------

/// Resolve a plain scalar under the JSON schema.
///
/// Dispatch order: null → bool → int → float.  No fallback — unmatched
/// scalars return `Err(UnresolvedScalar)`.
///
/// Note on `-0`: JSON int is `0 | -?[1-9][0-9]*`, so `-0` is not a JSON int
/// (the single-`0` branch is bare, with no sign).  JSON float is
/// `-?(0|[1-9][0-9]*)(\.[0-9]*)?([eE][-+]?[0-9]+)?`, so `-0` matches
/// (sign `-`, integer part `0`, no fractional or exponent).  Therefore `-0`
/// resolves to `Float` under the JSON schema.
fn resolve_json_plain(value: &str) -> Result<ResolvedTag, UnresolvedScalar> {
    if is_json_null(value) {
        Ok(ResolvedTag::Null)
    } else if is_json_bool(value) {
        Ok(ResolvedTag::Bool)
    } else if is_json_int(value) {
        Ok(ResolvedTag::Int)
    } else if is_json_float(value) {
        Ok(ResolvedTag::Float)
    } else {
        Err(UnresolvedScalar)
    }
}

// ---------------------------------------------------------------------------
// Core schema matchers (§10.3.2 tag resolution table)
// ---------------------------------------------------------------------------

/// `null | Null | NULL | ~ | ""` (YAML 1.2.2 §10.3.2 null row).
#[must_use]
pub fn is_core_null(value: &str) -> bool {
    matches!(value, "null" | "Null" | "NULL" | "~" | "")
}

/// `true | True | TRUE | false | False | FALSE` (§10.3.2 bool row).
#[must_use]
pub fn is_core_bool(value: &str) -> bool {
    matches!(
        value,
        "true" | "True" | "TRUE" | "false" | "False" | "FALSE"
    )
}

/// Decimal `[-+]?[0-9]+`, octal `0o[0-7]+`, hex `0x[0-9a-fA-F]+` (§10.3.2
/// int rows).  Leading zeros in decimal (e.g. `007`) are rejected.
#[must_use]
pub fn is_core_int(value: &str) -> bool {
    // Strip optional leading sign; the sign itself is never valid.
    let rest = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);

    if rest.is_empty() {
        return false;
    }

    if let Some(oct) = rest.strip_prefix("0o") {
        // Octal: must have at least one digit after prefix.
        !oct.is_empty() && oct.bytes().all(|b| matches!(b, b'0'..=b'7'))
    } else if let Some(hex) = rest.strip_prefix("0x") {
        // Hex: must have at least one digit after prefix.
        !hex.is_empty() && hex.bytes().all(|b| b.is_ascii_hexdigit())
    } else {
        // Decimal: no leading zeros unless the number is exactly "0".
        if rest.len() > 1 && rest.starts_with('0') {
            return false;
        }
        rest.bytes().all(|b| b.is_ascii_digit())
    }
}

/// Core float: decimal (`[-+]?(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`),
/// infinity (`[-+]?\.inf|\.Inf|\.INF`), not-a-number (`.nan|.NaN|.NAN`)
/// (§10.3.2 float rows).
#[must_use]
pub fn is_core_float(value: &str) -> bool {
    // Special values.
    if matches!(value, ".nan" | ".NaN" | ".NAN") {
        return true;
    }

    // Strip optional leading sign for inf and decimal.
    let unsigned = value
        .strip_prefix('-')
        .or_else(|| value.strip_prefix('+'))
        .unwrap_or(value);

    // Infinity: [+-]?.inf | .Inf | .INF
    if matches!(unsigned, ".inf" | ".Inf" | ".INF") {
        return true;
    }

    // Decimal float: (\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?
    is_core_decimal_float(unsigned)
}

/// Check whether `s` (already sign-stripped) matches the Core decimal float
/// pattern: `(\.[0-9]+|[0-9]+(\.[0-9]*)?)([eE][-+]?[0-9]+)?`.
fn is_core_decimal_float(s: &str) -> bool {
    // Split off optional exponent first.
    let (mantissa, exp_part) = split_exponent(s);

    // Validate exponent if present.
    if exp_part.is_some_and(|exp| !is_valid_exponent_digits(exp)) {
        return false;
    }

    // Mantissa must be either:
    //   a) \.[0-9]+  — leading-dot form
    //   b) [0-9]+(\.[0-9]*)?  — digit(s) with optional fractional part
    if let Some(after_dot) = mantissa.strip_prefix('.') {
        // Leading-dot form: must have at least one digit after the dot.
        !after_dot.is_empty() && after_dot.bytes().all(|b| b.is_ascii_digit())
    } else {
        // Digit-first form.
        let (int_part, frac) = mantissa.find('.').map_or((mantissa, None), |pos| {
            (&mantissa[..pos], Some(&mantissa[pos + 1..]))
        });
        if int_part.is_empty() || !int_part.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        // If there's a fractional part it may be empty (e.g. `1.`) or digits.
        if let Some(frac_digits) = frac {
            if !frac_digits.bytes().all(|b| b.is_ascii_digit()) {
                return false;
            }
        } else {
            // No dot at all — only valid if there's an exponent (e.g. `1e10`).
            // Without an exponent this is just an integer.
            if exp_part.is_none() {
                return false;
            }
        }
        true
    }
}

/// Split `s` at the first `e` or `E`, returning `(mantissa, Some(exponent_digits))`.
/// The exponent sign (`+`/`-`) is included in the returned exponent slice.
fn split_exponent(s: &str) -> (&str, Option<&str>) {
    s.find(['e', 'E'])
        .map_or((s, None), |pos| (&s[..pos], Some(&s[pos + 1..])))
}

/// Validate exponent digits: optional `+`/`-` followed by at least one ASCII digit.
fn is_valid_exponent_digits(exp: &str) -> bool {
    let digits = exp.strip_prefix(['-', '+']).unwrap_or(exp);
    !digits.is_empty() && digits.bytes().all(|b| b.is_ascii_digit())
}

// ---------------------------------------------------------------------------
// JSON schema matchers (§10.2.2 tag resolution table)
// ---------------------------------------------------------------------------

/// JSON null: exactly `"null"` (§10.2.2).
#[must_use]
pub fn is_json_null(value: &str) -> bool {
    value == "null"
}

/// JSON bool: `"true"` or `"false"` only (§10.2.2).
#[must_use]
pub fn is_json_bool(value: &str) -> bool {
    matches!(value, "true" | "false")
}

/// JSON int: `0 | -?[1-9][0-9]*` (§10.2.2).
///
/// No `+` sign, no octal, no hex, no leading zeros.
#[must_use]
pub fn is_json_int(value: &str) -> bool {
    if value == "0" {
        return true;
    }
    // -?[1-9][0-9]*
    let rest = value.strip_prefix('-').unwrap_or(value);
    let mut bytes = rest.bytes();
    match bytes.next() {
        // First digit must be 1–9.
        Some(b'1'..=b'9') => {}
        _ => return false,
    }
    bytes.all(|b| b.is_ascii_digit())
}

/// JSON float: `-?(0|[1-9][0-9]*)(\.[0-9]*)?([eE][-+]?[0-9]+)?` (§10.2.2).
///
/// No `+` sign, no leading-dot form, no `.inf`, no `.nan`.
#[must_use]
pub fn is_json_float(value: &str) -> bool {
    // Strip optional leading minus (no + allowed).
    let unsigned = value.strip_prefix('-').unwrap_or(value);

    // Integer part: `0` or `[1-9][0-9]*`.
    let after_int = if let Some(rest) = unsigned.strip_prefix('0') {
        rest
    } else {
        let mut bytes = unsigned.bytes();
        match bytes.next() {
            Some(b'1'..=b'9') => {}
            _ => return false,
        }
        let consumed = 1 + bytes.take_while(u8::is_ascii_digit).count();
        &unsigned[consumed..]
    };

    // Optional fractional part: `\.[0-9]*`
    let after_frac = after_int.strip_prefix('.').map_or(after_int, |rest| {
        let digits = rest.bytes().take_while(u8::is_ascii_digit).count();
        &rest[digits..]
    });

    // Optional exponent: `[eE][-+]?[0-9]+`
    let after_exp = if let Some(exp_rest) = after_frac
        .strip_prefix('e')
        .or_else(|| after_frac.strip_prefix('E'))
    {
        let digits_start = exp_rest.strip_prefix(['-', '+']).unwrap_or(exp_rest);
        if digits_start.is_empty() || !digits_start.bytes().all(|b| b.is_ascii_digit()) {
            return false;
        }
        ""
    } else {
        after_frac
    };

    // Must have consumed the entire string.
    after_exp.is_empty()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Chomp;
    use rstest::rstest;

    // ── 1. ResolvedTag::as_str() ───────────────────────────────────────────

    #[rstest]
    #[case::str_tag(ResolvedTag::Str, "tag:yaml.org,2002:str")]
    #[case::int_tag(ResolvedTag::Int, "tag:yaml.org,2002:int")]
    #[case::float_tag(ResolvedTag::Float, "tag:yaml.org,2002:float")]
    #[case::bool_tag(ResolvedTag::Bool, "tag:yaml.org,2002:bool")]
    #[case::null_tag(ResolvedTag::Null, "tag:yaml.org,2002:null")]
    #[case::seq_tag(ResolvedTag::Seq, "tag:yaml.org,2002:seq")]
    #[case::map_tag(ResolvedTag::Map, "tag:yaml.org,2002:map")]
    fn resolved_tag_as_str_returns_uri(#[case] tag: ResolvedTag, #[case] expected: &str) {
        assert_eq!(tag.as_str(), expected);
    }

    // ── 2. Core regex matchers ─────────────────────────────────────────────

    // is_core_null — true

    #[rstest]
    #[case::null_lowercase("null")]
    #[case::null_titlecase("Null")]
    #[case::null_uppercase("NULL")]
    #[case::tilde("~")]
    #[case::empty("")]
    fn is_core_null_returns_true(#[case] input: &str) {
        assert!(is_core_null(input));
    }

    // is_core_null — false

    #[rstest]
    #[case::none_string("none")]
    #[case::nil_string("nil")]
    #[case::mixed_case_null("nUll")]
    #[case::single_space(" ")]
    #[case::json_null_inside_word("nullX")]
    fn is_core_null_returns_false(#[case] input: &str) {
        assert!(!is_core_null(input));
    }

    // is_core_bool — true

    #[rstest]
    #[case::true_lowercase("true")]
    #[case::true_titlecase("True")]
    #[case::true_uppercase("TRUE")]
    #[case::false_lowercase("false")]
    #[case::false_titlecase("False")]
    #[case::false_uppercase("FALSE")]
    fn is_core_bool_returns_true(#[case] input: &str) {
        assert!(is_core_bool(input));
    }

    // is_core_bool — false

    #[rstest]
    #[case::yaml11_yes("yes")]
    #[case::yaml11_no("no")]
    #[case::yaml11_on("on")]
    #[case::yaml11_off("off")]
    #[case::mixed_case_true("tRue")]
    #[case::integer_one("1")]
    #[case::integer_zero("0")]
    fn is_core_bool_returns_false(#[case] input: &str) {
        assert!(!is_core_bool(input));
    }

    // is_core_int — true

    #[rstest]
    #[case::decimal_zero("0")]
    #[case::decimal_positive("42")]
    #[case::decimal_negative("-1")]
    #[case::decimal_plus_prefix("+100")]
    #[case::octal("0o17")]
    #[case::octal_negative("-0o10")]
    #[case::hex_lower("0xff")]
    #[case::hex_upper("0xFF")]
    #[case::hex_negative("-0x1A")]
    fn is_core_int_returns_true(#[case] input: &str) {
        assert!(is_core_int(input));
    }

    // is_core_int — false

    #[rstest]
    #[case::leading_zeros("007")]
    #[case::empty("")]
    #[case::sign_only_plus("+")]
    #[case::sign_only_minus("-")]
    #[case::float_with_dot("3.14")]
    #[case::float_exp("1e5")]
    #[case::octal_prefix_only("0o")]
    #[case::hex_prefix_only("0x")]
    #[case::alpha_string("abc")]
    fn is_core_int_returns_false(#[case] input: &str) {
        assert!(!is_core_int(input));
    }

    // is_core_float — true

    #[rstest]
    #[case::decimal_dot("3.14")]
    #[case::decimal_no_integer_part(".5")]
    #[case::exponent_only("1e10")]
    #[case::exponent_negative("1.5E-3")]
    #[case::positive_signed_float("+1.0")]
    #[case::negative_float("-0.5")]
    #[case::inf_lowercase(".inf")]
    #[case::inf_titlecase(".Inf")]
    #[case::inf_uppercase(".INF")]
    #[case::neg_inf_lowercase("-.inf")]
    #[case::neg_inf_titlecase("-.Inf")]
    #[case::neg_inf_uppercase("-.INF")]
    #[case::pos_inf("+.inf")]
    #[case::nan_lowercase(".nan")]
    #[case::nan_titlecase(".NaN")]
    #[case::nan_uppercase(".NAN")]
    fn is_core_float_returns_true(#[case] input: &str) {
        assert!(is_core_float(input));
    }

    // is_core_float — false

    #[rstest]
    #[case::bare_integer("42")]
    #[case::empty("")]
    #[case::bare_inf_no_dot("inf")]
    #[case::bare_nan_no_dot("nan")]
    #[case::sign_only("+")]
    #[case::dot_only(".")]
    fn is_core_float_returns_false(#[case] input: &str) {
        assert!(!is_core_float(input));
    }

    // ── 3. JSON regex matchers ─────────────────────────────────────────────

    // is_json_null

    #[test]
    fn is_json_null_returns_true() {
        assert!(is_json_null("null"));
    }

    #[rstest]
    #[case::null_titlecase("Null")]
    #[case::null_uppercase("NULL")]
    #[case::tilde("~")]
    #[case::empty("")]
    fn is_json_null_returns_false(#[case] input: &str) {
        assert!(!is_json_null(input));
    }

    // is_json_bool

    #[rstest]
    #[case::true_lowercase("true")]
    #[case::false_lowercase("false")]
    fn is_json_bool_returns_true(#[case] input: &str) {
        assert!(is_json_bool(input));
    }

    #[rstest]
    #[case::true_titlecase("True")]
    #[case::true_uppercase("TRUE")]
    #[case::false_titlecase("False")]
    #[case::false_uppercase("FALSE")]
    fn is_json_bool_returns_false(#[case] input: &str) {
        assert!(!is_json_bool(input));
    }

    // is_json_int

    #[rstest]
    #[case::zero("0")]
    #[case::positive_decimal("42")]
    #[case::negative_decimal("-1")]
    #[case::negative_multi("-100")]
    #[case::large_negative("-9999")]
    fn is_json_int_returns_true(#[case] input: &str) {
        assert!(is_json_int(input));
    }

    #[rstest]
    #[case::plus_prefix("+42")]
    #[case::plus_zero("+0")]
    #[case::minus_zero("-0")]
    #[case::leading_zeros("007")]
    #[case::octal("0o17")]
    #[case::hex("0xFF")]
    #[case::empty("")]
    #[case::sign_only_plus("+")]
    #[case::sign_only_minus("-")]
    fn is_json_int_returns_false(#[case] input: &str) {
        assert!(!is_json_int(input));
    }

    // is_json_float

    #[rstest]
    #[case::zero_float_simple("0.5")]
    #[case::negative_with_decimal("-1.5")]
    #[case::with_exponent("1e10")]
    #[case::with_negative_exponent("-1.5e-3")]
    // `-0` matches `-?(0)` with no fractional/exponent — valid JSON float.
    #[case::minus_zero("-0")]
    // bare `0` matches the integer part with no fractional or exponent.
    #[case::zero_alone("0")]
    fn is_json_float_returns_true(#[case] input: &str) {
        assert!(is_json_float(input));
    }

    #[rstest]
    #[case::plus_prefix("+1.5")]
    #[case::inf_dot(".inf")]
    #[case::nan_dot(".nan")]
    #[case::leading_dot(".5")]
    #[case::empty("")]
    #[case::sign_only("-")]
    fn is_json_float_returns_false(#[case] input: &str) {
        assert!(!is_json_float(input));
    }

    // ── 4. resolve_scalar ─────────────────────────────────────────────────

    // 4a. Failsafe schema

    #[rstest]
    #[case::plain_null(ScalarStyle::Plain, "null", None)]
    #[case::single_quoted_true(ScalarStyle::SingleQuoted, "true", None)]
    #[case::double_quoted_int(ScalarStyle::DoubleQuoted, "42", None)]
    #[case::literal_block(ScalarStyle::Literal(Chomp::Clip), "hello", None)]
    #[case::folded_block(ScalarStyle::Folded(Chomp::Strip), "world", None)]
    fn resolve_scalar_failsafe_always_str(
        #[case] style: ScalarStyle,
        #[case] value: &str,
        #[case] source_tag: Option<&str>,
    ) {
        assert_eq!(
            resolve_scalar(Schema::Failsafe, style, value, source_tag),
            Ok(Some(ResolvedTag::Str))
        );
    }

    #[test]
    fn resolve_scalar_failsafe_explicit_tag_passthrough() {
        let result = resolve_scalar(
            Schema::Failsafe,
            ScalarStyle::Plain,
            "null",
            Some("tag:yaml.org,2002:str"),
        );
        assert_eq!(result, Ok(None));
    }

    // 4b. Core schema

    #[rstest]
    #[case::plain_null_lowercase(ScalarStyle::Plain, "null", None, ResolvedTag::Null)]
    #[case::plain_null_tilde(ScalarStyle::Plain, "~", None, ResolvedTag::Null)]
    #[case::plain_null_empty(ScalarStyle::Plain, "", None, ResolvedTag::Null)]
    #[case::plain_bool_true_lower(ScalarStyle::Plain, "true", None, ResolvedTag::Bool)]
    #[case::plain_bool_false_upper(ScalarStyle::Plain, "FALSE", None, ResolvedTag::Bool)]
    #[case::plain_int_decimal(ScalarStyle::Plain, "42", None, ResolvedTag::Int)]
    #[case::plain_int_octal(ScalarStyle::Plain, "0o17", None, ResolvedTag::Int)]
    #[case::plain_int_hex(ScalarStyle::Plain, "0xFF", None, ResolvedTag::Int)]
    #[case::plain_float_decimal(ScalarStyle::Plain, "3.14", None, ResolvedTag::Float)]
    #[case::plain_float_inf(ScalarStyle::Plain, ".inf", None, ResolvedTag::Float)]
    #[case::plain_float_nan(ScalarStyle::Plain, ".nan", None, ResolvedTag::Float)]
    #[case::plain_unmatched_str(ScalarStyle::Plain, "hello", None, ResolvedTag::Str)]
    #[case::plain_leading_zeros(ScalarStyle::Plain, "007", None, ResolvedTag::Str)]
    #[case::single_quoted_null(ScalarStyle::SingleQuoted, "null", None, ResolvedTag::Str)]
    #[case::double_quoted_true(ScalarStyle::DoubleQuoted, "true", None, ResolvedTag::Str)]
    #[case::literal_any(ScalarStyle::Literal(Chomp::Clip), "42", None, ResolvedTag::Str)]
    #[case::folded_any(ScalarStyle::Folded(Chomp::Keep), "null", None, ResolvedTag::Str)]
    fn resolve_scalar_core(
        #[case] style: ScalarStyle,
        #[case] value: &str,
        #[case] source_tag: Option<&str>,
        #[case] expected: ResolvedTag,
    ) {
        assert_eq!(
            resolve_scalar(Schema::Core, style, value, source_tag),
            Ok(Some(expected))
        );
    }

    #[test]
    fn resolve_scalar_core_explicit_tag_passthrough() {
        let result = resolve_scalar(
            Schema::Core,
            ScalarStyle::Plain,
            "null",
            Some("tag:yaml.org,2002:int"),
        );
        assert_eq!(result, Ok(None));
    }

    // 4c. JSON schema

    #[rstest]
    // null
    #[case::plain_null_lowercase(ScalarStyle::Plain, "null", None, Ok(Some(ResolvedTag::Null)))]
    // JSON rejects Core-only null forms
    #[case::plain_null_tilde_rejected(ScalarStyle::Plain, "~", None, Err(UnresolvedScalar))]
    #[case::plain_empty_rejected(ScalarStyle::Plain, "", None, Err(UnresolvedScalar))]
    // bool
    #[case::plain_bool_true_lower(ScalarStyle::Plain, "true", None, Ok(Some(ResolvedTag::Bool)))]
    #[case::plain_bool_true_upper_rejected(ScalarStyle::Plain, "TRUE", None, Err(UnresolvedScalar))]
    // int
    #[case::plain_int_decimal(ScalarStyle::Plain, "42", None, Ok(Some(ResolvedTag::Int)))]
    #[case::plain_int_zero(ScalarStyle::Plain, "0", None, Ok(Some(ResolvedTag::Int)))]
    #[case::plain_int_negative(ScalarStyle::Plain, "-1", None, Ok(Some(ResolvedTag::Int)))]
    #[case::plain_int_plus_rejected(ScalarStyle::Plain, "+42", None, Err(UnresolvedScalar))]
    // -0: not a JSON int; dispatched to float (matches `-?(0)` with no fractional/exp)
    #[case::plain_minus_zero_is_float(ScalarStyle::Plain, "-0", None, Ok(Some(ResolvedTag::Float)))]
    #[case::plain_octal_rejected(ScalarStyle::Plain, "0o17", None, Err(UnresolvedScalar))]
    #[case::plain_hex_rejected(ScalarStyle::Plain, "0xFF", None, Err(UnresolvedScalar))]
    // float
    #[case::plain_float_decimal(ScalarStyle::Plain, "1.5", None, Ok(Some(ResolvedTag::Float)))]
    #[case::plain_float_inf_rejected(ScalarStyle::Plain, ".inf", None, Err(UnresolvedScalar))]
    #[case::plain_float_nan_rejected(ScalarStyle::Plain, ".nan", None, Err(UnresolvedScalar))]
    #[case::plain_float_plus_rejected(ScalarStyle::Plain, "+1.5", None, Err(UnresolvedScalar))]
    // unmatched
    #[case::plain_unmatched_rejected(ScalarStyle::Plain, "hello", None, Err(UnresolvedScalar))]
    // non-plain styles → Str (no pattern matching)
    #[case::single_quoted_becomes_str(
        ScalarStyle::SingleQuoted,
        "null",
        None,
        Ok(Some(ResolvedTag::Str))
    )]
    #[case::double_quoted_becomes_str(
        ScalarStyle::DoubleQuoted,
        "true",
        None,
        Ok(Some(ResolvedTag::Str))
    )]
    #[case::literal_becomes_str(
        ScalarStyle::Literal(Chomp::Clip),
        "42",
        None,
        Ok(Some(ResolvedTag::Str))
    )]
    #[case::folded_becomes_str(
        ScalarStyle::Folded(Chomp::Strip),
        "null",
        None,
        Ok(Some(ResolvedTag::Str))
    )]
    fn resolve_scalar_json(
        #[case] style: ScalarStyle,
        #[case] value: &str,
        #[case] source_tag: Option<&str>,
        #[case] expected: Result<Option<ResolvedTag>, UnresolvedScalar>,
    ) {
        assert_eq!(
            resolve_scalar(Schema::Json, style, value, source_tag),
            expected
        );
    }

    #[test]
    fn resolve_scalar_json_explicit_tag_passthrough() {
        let result = resolve_scalar(Schema::Json, ScalarStyle::Plain, "null", Some("!custom"));
        assert_eq!(result, Ok(None));
    }

    // 4d. source_tag passthrough — cross-schema

    #[test]
    fn resolve_scalar_explicit_tag_returns_none_failsafe() {
        assert_eq!(
            resolve_scalar(
                Schema::Failsafe,
                ScalarStyle::Plain,
                "null",
                Some("anything")
            ),
            Ok(None)
        );
    }

    #[test]
    fn resolve_scalar_explicit_tag_returns_none_json() {
        assert_eq!(
            resolve_scalar(Schema::Json, ScalarStyle::Plain, "null", Some("anything")),
            Ok(None)
        );
    }

    #[test]
    fn resolve_scalar_explicit_tag_returns_none_core() {
        assert_eq!(
            resolve_scalar(Schema::Core, ScalarStyle::Plain, "null", Some("anything")),
            Ok(None)
        );
    }

    // ── 5. resolve_collection ─────────────────────────────────────────────

    #[rstest]
    #[case::failsafe_sequence_no_tag(
        Schema::Failsafe,
        CollectionKind::Sequence,
        None,
        Some(ResolvedTag::Seq)
    )]
    #[case::failsafe_mapping_no_tag(
        Schema::Failsafe,
        CollectionKind::Mapping,
        None,
        Some(ResolvedTag::Map)
    )]
    #[case::json_sequence_no_tag(
        Schema::Json,
        CollectionKind::Sequence,
        None,
        Some(ResolvedTag::Seq)
    )]
    #[case::json_mapping_no_tag(
        Schema::Json,
        CollectionKind::Mapping,
        None,
        Some(ResolvedTag::Map)
    )]
    #[case::core_sequence_no_tag(
        Schema::Core,
        CollectionKind::Sequence,
        None,
        Some(ResolvedTag::Seq)
    )]
    #[case::core_mapping_no_tag(
        Schema::Core,
        CollectionKind::Mapping,
        None,
        Some(ResolvedTag::Map)
    )]
    #[case::failsafe_sequence_explicit_tag(
        Schema::Failsafe,
        CollectionKind::Sequence,
        Some("!custom"),
        None
    )]
    #[case::failsafe_mapping_explicit_tag(
        Schema::Failsafe,
        CollectionKind::Mapping,
        Some("tag:yaml.org,2002:map"),
        None
    )]
    #[case::core_sequence_explicit_tag(Schema::Core, CollectionKind::Sequence, Some("!seq"), None)]
    #[case::json_mapping_explicit_tag(Schema::Json, CollectionKind::Mapping, Some("!map"), None)]
    fn resolve_collection_dispatch(
        #[case] schema: Schema,
        #[case] kind: CollectionKind,
        #[case] source_tag: Option<&str>,
        #[case] expected: Option<ResolvedTag>,
    ) {
        assert_eq!(resolve_collection(schema, kind, source_tag), expected);
    }
}
