// SPDX-License-Identifier: MIT

/// Unused-anchor and unresolved-alias validator.
pub mod anchors;
/// Custom tag types and parser.
pub mod custom_tag;
/// Custom tag validator.
pub mod custom_tags_validation;
/// Duplicate mapping key validator.
pub mod duplicate_keys;
/// Flow style validator.
pub mod flow_style;
/// Map key ordering validator.
pub mod key_ordering;
/// YAML 1.1 compatibility validator.
pub mod yaml11_compat;

pub use anchors::validate_unused_anchors;
pub use custom_tag::{CustomTag, TagNodeType, parse_custom_tag};
pub use custom_tags_validation::validate_custom_tags;
pub use duplicate_keys::validate_duplicate_keys;
pub use flow_style::validate_flow_style;
pub use key_ordering::validate_key_ordering;
pub use yaml11_compat::validate_yaml11_compat;

/// Longest name, in characters, that a validator message shows in full.
const MAX_DISPLAYED_NAME_CHARS: usize = 100;

/// Shorten an anchor or key name for a diagnostic message: keep at most
/// [`MAX_DISPLAYED_NAME_CHARS`] characters and append `...` only when
/// something was cut. Counts characters, not bytes, and stops after the
/// first cut position, so a huge name costs no more than a short one.
pub(super) fn truncate_name(name: &str) -> String {
    match name.char_indices().nth(MAX_DISPLAYED_NAME_CHARS) {
        Some((cut, _)) => format!("{}...", &name[..cut]),
        None => name.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::truncate_name;

    #[rstest]
    #[case::empty(String::new(), String::new())]
    #[case::ascii_99("x".repeat(99), "x".repeat(99))]
    #[case::ascii_100("x".repeat(100), "x".repeat(100))]
    #[case::ascii_101("x".repeat(101), format!("{}...", "x".repeat(100)))]
    #[case::two_byte_60_chars("é".repeat(60), "é".repeat(60))]
    #[case::two_byte_100_chars("é".repeat(100), "é".repeat(100))]
    #[case::two_byte_101_chars("é".repeat(101), format!("{}...", "é".repeat(100)))]
    #[case::three_byte_150_chars_byte_100_inside_char("€".repeat(150), format!("{}...", "€".repeat(100)))]
    #[case::ascii_then_two_byte_150_chars_byte_100_inside_char(
        format!("a{}", "é".repeat(150)),
        format!("a{}...", "é".repeat(99))
    )]
    #[case::four_byte_101_chars("😀".repeat(101), format!("{}...", "😀".repeat(100)))]
    fn truncate_name_cases(#[case] name: String, #[case] expected: String) {
        assert_eq!(truncate_name(&name), expected);
    }
}
