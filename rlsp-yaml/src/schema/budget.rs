// SPDX-License-Identifier: MIT

//! Memory budgets of one schema parse.
//!
//! A schema comes from a URL that a document names, so everything the parse
//! allocates must be bounded by something other than the size of one response:
//! a 5 MiB document deserializes to hundreds of MiB, and a parse reads up to 21
//! of them. A parse therefore has two budgets, both in estimated heap bytes:
//!
//! - the **raw** budget covers the deserialized documents the parse reads, and
//! - the **parsed** budget covers everything the parse builds from them: the
//!   schema nodes and the tables that find the targets of `$ref`s.
//!
//! The parse charges an estimate before it makes an allocation that grows with
//! the input, and stops as soon as a budget is exceeded. A parse that exceeds a
//! budget fails as a whole, so no part of a schema is ever applied.

use serde_json::Value;

/// Budget of the schema built by one parse, in estimated heap bytes (256 MiB).
pub(super) const PARSED_BUDGET: usize = 256 << 20;

/// Budget of the deserialized documents read by one parse, in estimated heap
/// bytes (128 MiB).
pub(super) const RAW_BUDGET: usize = 128 << 20;

// Upper bounds of the heap that `serde_json::Value` takes per token of a JSON
// text, at its peak while the text is deserialized, with glibc chunk sizes: a
// map is a `BTreeMap`, whose one-entry node is about 640 B and whose entries
// cost up to 128 B in half-full nodes, and a non-empty array allocates at least
// four 32 B slots. They were checked against a counting allocator on documents
// of each shape.

/// An object, charged at its `{`.
const OBJECT_BYTES: usize = 700;
/// An array, charged at its `[`.
const ARRAY_BYTES: usize = 160;
/// An entry of an object, charged at its `:`.
const ENTRY_BYTES: usize = 128;
/// An element after the first of an array or object, charged at its `,`. An
/// array grows by doubling and holds the old and the new buffer while it does:
/// up to three 32 B slots for each element that it ends up with.
const SEPARATOR_BYTES: usize = 96;
/// A string, charged at its opening quote; its bytes are charged one each.
const STRING_BYTES: usize = 32;
/// The slot that holds the value at the root of a document.
const ROOT_SLOT_BYTES: usize = 64;

/// The budget of a parse that a [`SchemaError::TooLarge`](super::SchemaError)
/// reports as exceeded.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SizeBudget {
    /// The schema built by the parse.
    Parsed,
    /// The deserialized documents the parse reads.
    Raw,
}

/// A running total of estimated bytes against a limit.
#[derive(Debug)]
pub(super) struct Meter {
    limit: usize,
    spent: usize,
}

impl Meter {
    pub(super) const fn new(limit: usize) -> Self {
        Self { limit, spent: 0 }
    }

    /// Add `bytes` to the total; `false` once the total exceeds the limit,
    /// which it then does for good.
    pub(super) const fn charge(&mut self, bytes: usize) -> bool {
        self.spent = self.spent.saturating_add(bytes);
        !self.is_over()
    }

    pub(super) const fn is_over(&self) -> bool {
        self.spent > self.limit
    }

    #[cfg(test)]
    pub(super) const fn spent(&self) -> usize {
        self.spent
    }
}

/// The two budgets of one parse.
#[derive(Debug)]
pub(super) struct Meters {
    pub(super) parsed: Meter,
    pub(super) raw: Meter,
    /// How many schema nodes the parse charged, for the tests.
    #[cfg(test)]
    pub(super) nodes: usize,
    /// What the parsed budget had been charged when the first schema node was,
    /// which is what the discovery of the targets cost, for the tests.
    #[cfg(test)]
    pub(super) spent_before_first_node: usize,
}

impl Meters {
    pub(super) const fn new(parsed_limit: usize, raw_limit: usize) -> Self {
        Self {
            parsed: Meter::new(parsed_limit),
            raw: Meter::new(raw_limit),
            #[cfg(test)]
            nodes: 0,
            #[cfg(test)]
            spent_before_first_node: 0,
        }
    }

    pub(super) const fn production() -> Self {
        Self::new(PARSED_BUDGET, RAW_BUDGET)
    }

    /// Whether either budget is exceeded.
    pub(super) const fn is_over(&self) -> bool {
        self.parsed.is_over() || self.raw.is_over()
    }

    /// The budget that is exceeded, the parsed one first.
    pub(super) const fn exceeded(&self) -> Option<SizeBudget> {
        if self.parsed.is_over() {
            Some(SizeBudget::Parsed)
        } else if self.raw.is_over() {
            Some(SizeBudget::Raw)
        } else {
            None
        }
    }
}

/// Estimate the heap of the `Value` that deserializing `text` makes, from the
/// text alone: a count of its tokens, so it runs before anything is allocated.
///
/// The scan follows strings and their escapes, so an object or a comma inside
/// a string costs one byte like any other byte of it. For JSON text that
/// `serde_json` accepts, the result is never below [`estimate_value`] of the
/// value it deserializes to, and is equal when no string holds an escape.
pub(super) fn estimate_bytes(text: &[u8]) -> usize {
    let mut total = ROOT_SLOT_BYTES;
    let mut in_string = false;
    let mut escaped = false;
    for &byte in text {
        let cost = if in_string {
            match (escaped, byte) {
                (true, _) => {
                    escaped = false;
                    1
                }
                (false, b'\\') => {
                    escaped = true;
                    1
                }
                (false, b'"') => {
                    in_string = false;
                    0
                }
                (false, _) => 1,
            }
        } else {
            match byte {
                b'{' => OBJECT_BYTES,
                b'[' => ARRAY_BYTES,
                b':' => ENTRY_BYTES,
                b',' => SEPARATOR_BYTES,
                b'"' => {
                    in_string = true;
                    STRING_BYTES
                }
                _ => 0,
            }
        };
        total = total.saturating_add(cost);
    }
    total
}

/// Estimate the heap of `value` with the weights of [`estimate_bytes`], for a
/// document that is already deserialized and for a value that the parse copies.
///
/// Recursive: `value` must not nest deeper than the parse allows.
pub(super) fn estimate_value(value: &Value) -> usize {
    ROOT_SLOT_BYTES.saturating_add(estimate_below_root(value))
}

fn estimate_below_root(value: &Value) -> usize {
    let separators = |count: usize| SEPARATOR_BYTES.saturating_mul(count.saturating_sub(1));
    match value {
        Value::Null | Value::Bool(_) | Value::Number(_) => 0,
        Value::String(text) => STRING_BYTES.saturating_add(text.len()),
        Value::Array(items) => items.iter().fold(
            ARRAY_BYTES.saturating_add(separators(items.len())),
            |total, item| total.saturating_add(estimate_below_root(item)),
        ),
        Value::Object(map) => map.iter().fold(
            OBJECT_BYTES.saturating_add(separators(map.len())),
            |total, (key, member)| {
                total
                    .saturating_add(ENTRY_BYTES)
                    .saturating_add(STRING_BYTES)
                    .saturating_add(key.len())
                    .saturating_add(estimate_below_root(member))
            },
        ),
    }
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use rstest::rstest;
    use serde_json::json;

    use super::*;

    #[rstest]
    #[case::null("null", ROOT_SLOT_BYTES)]
    #[case::number("1.5e10", ROOT_SLOT_BYTES)]
    #[case::empty_string(r#""""#, ROOT_SLOT_BYTES + STRING_BYTES)]
    #[case::string(r#""abc""#, ROOT_SLOT_BYTES + STRING_BYTES + 3)]
    #[case::empty_array("[]", ROOT_SLOT_BYTES + ARRAY_BYTES)]
    #[case::array_of_two("[0,0]", ROOT_SLOT_BYTES + ARRAY_BYTES + SEPARATOR_BYTES)]
    #[case::empty_object("{}", ROOT_SLOT_BYTES + OBJECT_BYTES)]
    #[case::one_entry_object(
        r#"{"a":0}"#,
        ROOT_SLOT_BYTES + OBJECT_BYTES + ENTRY_BYTES + STRING_BYTES + 1
    )]
    #[case::two_entry_object(
        r#"{"a":0,"b":0}"#,
        ROOT_SLOT_BYTES + OBJECT_BYTES + 2 * (ENTRY_BYTES + STRING_BYTES + 1) + SEPARATOR_BYTES
    )]
    #[case::nested(
        r#"{"a":[{"b":"c"}]}"#,
        ROOT_SLOT_BYTES
            + OBJECT_BYTES + ENTRY_BYTES + STRING_BYTES + 1
            + ARRAY_BYTES
            + OBJECT_BYTES + ENTRY_BYTES + STRING_BYTES + 1 + STRING_BYTES + 1
    )]
    #[case::whitespace_is_free("  [ 0 ,\n 0 ]  ", ROOT_SLOT_BYTES + ARRAY_BYTES + SEPARATOR_BYTES)]
    fn estimates_equal_for_text_without_escapes(#[case] text: &str, #[case] expected: usize) {
        let value: Value = serde_json::from_str(text).expect("JSON");

        assert_eq!(estimate_bytes(text.as_bytes()), expected);
        assert_eq!(estimate_value(&value), expected);
    }

    #[rstest]
    #[case::brace_in_string(r#"["{{{{"]"#)]
    #[case::commas_in_string(r#"["a,b,c,d"]"#)]
    #[case::colon_and_bracket_in_string(r#"["a:[b"]"#)]
    #[case::escaped_quote_does_not_end_the_string(r#"["a\"{,b"]"#)]
    #[case::escaped_backslash_ends_before_the_quote(r#"["a\\","{"]"#)]
    fn structural_bytes_inside_a_string_cost_one_byte_each(#[case] text: &str) {
        let value: Value = serde_json::from_str(text).expect("JSON");

        assert!(estimate_bytes(text.as_bytes()) >= estimate_value(&value));
        // A string of `{` is not charged as objects.
        assert!(estimate_bytes(text.as_bytes()) < ROOT_SLOT_BYTES + OBJECT_BYTES);
    }

    #[test]
    fn estimate_of_nested_arrays_adds_every_level() {
        let nested = Value::Array(vec![json!([0, 0]); 3]);

        assert_eq!(
            estimate_value(&nested),
            ROOT_SLOT_BYTES
                + ARRAY_BYTES
                + 2 * SEPARATOR_BYTES
                + 3 * (ARRAY_BYTES + SEPARATOR_BYTES)
        );
    }

    #[test]
    fn meter_stays_over_once_exceeded() {
        let mut meter = Meter::new(10);

        assert!(meter.charge(10));
        assert!(!meter.charge(1));
        assert!(meter.is_over());
        assert!(!meter.charge(0));
    }

    #[test]
    fn meter_with_the_largest_limit_never_overflows() {
        let mut meter = Meter::new(usize::MAX);

        assert!(meter.charge(usize::MAX));
        assert!(meter.charge(usize::MAX));
        assert!(!meter.is_over());
    }

    #[test]
    fn meters_report_the_parsed_budget_before_the_raw_one() {
        let mut meters = Meters::new(1, 1);
        assert_eq!(meters.exceeded(), None);

        meters.raw.charge(2);
        assert_eq!(meters.exceeded(), Some(SizeBudget::Raw));

        meters.parsed.charge(2);
        assert_eq!(meters.exceeded(), Some(SizeBudget::Parsed));
    }

    #[test]
    fn production_budgets_are_256_and_128_mib() {
        assert_eq!(PARSED_BUDGET, 256 * 1024 * 1024);
        assert_eq!(RAW_BUDGET, 128 * 1024 * 1024);
    }

    fn json_value() -> impl Strategy<Value = Value> {
        let text = prop_oneof![
            "[a-z{}\\[\\],:]{0,8}",
            Just("a\"b\\c\n\u{1F600}".to_string()),
            Just(String::new()),
        ];
        let leaf = prop_oneof![
            Just(Value::Null),
            any::<bool>().prop_map(Value::Bool),
            any::<i32>().prop_map(|n| json!(n)),
            text.clone().prop_map(Value::String),
        ];
        leaf.prop_recursive(5, 64, 6, move |inner| {
            prop_oneof![
                prop::collection::vec(inner.clone(), 0..6).prop_map(Value::Array),
                prop::collection::btree_map(text.clone(), inner, 0..6)
                    .prop_map(|map| Value::Object(map.into_iter().collect())),
            ]
        })
    }

    proptest! {
        #![proptest_config(ProptestConfig {
            cases: 64,
            ..ProptestConfig::default()
        })]

        /// The estimate from the text never falls below the estimate from the
        /// value, so a document is not accepted from the network and then
        /// rejected from the cache, or the other way round.
        #[test]
        fn estimate_from_text_is_never_below_estimate_from_value(value in json_value()) {
            let compact = serde_json::to_string(&value).expect("serializes");
            let pretty = serde_json::to_string_pretty(&value).expect("serializes");

            let from_value = estimate_value(&value);

            prop_assert!(estimate_bytes(compact.as_bytes()) >= from_value);
            prop_assert_eq!(estimate_bytes(compact.as_bytes()), estimate_bytes(pretty.as_bytes()));
        }
    }
}
