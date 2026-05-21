// SPDX-License-Identifier: MIT

/// The expected YAML node type for a custom tag.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TagNodeType {
    /// The tagged node must be a scalar.
    Scalar,
    /// The tagged node must be a mapping.
    Mapping,
    /// The tagged node must be a sequence.
    Sequence,
}

/// A parsed custom tag entry, optionally carrying a node-type annotation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomTag {
    /// The tag name, e.g. `"!include"`.
    pub name: String,
    /// The expected node type, if specified.
    pub expected_type: Option<TagNodeType>,
}

/// Parse a custom tag string into a `CustomTag`.
///
/// The input is the raw string from the `customTags` setting or modeline, e.g.
/// `"!include scalar"`, `"!Ref mapping"`, or `"!bare"`.
///
/// - If the string ends with a single space followed by `scalar`, `mapping`, or
///   `sequence` (case-insensitive), the suffix is parsed as the expected type and
///   the tag name is the remainder.
/// - Any other suffix (unknown type word, double space, etc.) leaves the entire
///   string as the tag name with no type annotation, preserving backward
///   compatibility.
#[must_use]
pub fn parse_custom_tag(input: &str) -> CustomTag {
    if let Some(space_pos) = input.rfind(' ') {
        let suffix = &input[space_pos + 1..];
        let node_type = match suffix.to_ascii_lowercase().as_str() {
            "scalar" => Some(TagNodeType::Scalar),
            "mapping" => Some(TagNodeType::Mapping),
            "sequence" => Some(TagNodeType::Sequence),
            _ => None,
        };
        if let Some(expected_type) = node_type {
            // Only split if the preceding part is a single space (not double-space).
            // rfind gives us the last space; we need exactly one space at that position.
            let prefix = &input[..space_pos];
            // Ensure prefix doesn't end with a space (would mean double-space before suffix).
            if !prefix.ends_with(' ') {
                return CustomTag {
                    name: prefix.to_string(),
                    expected_type: Some(expected_type),
                };
            }
        }
    }
    CustomTag {
        name: input.to_string(),
        expected_type: None,
    }
}

#[cfg(test)]
mod tests {
    use rstest::rstest;

    use super::*;

    #[test]
    fn parse_custom_tag_scalar_suffix_recognized() {
        let result = parse_custom_tag("!include scalar");
        assert_eq!(result.name, "!include");
        assert_eq!(result.expected_type, Some(TagNodeType::Scalar));
    }

    #[test]
    fn parse_custom_tag_mapping_suffix_recognized() {
        let result = parse_custom_tag("!ref mapping");
        assert_eq!(result.name, "!ref");
        assert_eq!(result.expected_type, Some(TagNodeType::Mapping));
    }

    #[test]
    fn parse_custom_tag_sequence_suffix_recognized() {
        let result = parse_custom_tag("!env sequence");
        assert_eq!(result.name, "!env");
        assert_eq!(result.expected_type, Some(TagNodeType::Sequence));
    }

    #[test]
    fn parse_custom_tag_no_suffix_produces_none() {
        let result = parse_custom_tag("!include");
        assert_eq!(result.name, "!include");
        assert_eq!(result.expected_type, None);
    }

    #[test]
    fn parse_custom_tag_unknown_suffix_becomes_name() {
        let result = parse_custom_tag("!include blob");
        assert_eq!(result.name, "!include blob");
        assert_eq!(result.expected_type, None);
    }

    #[rstest]
    #[case("!include SCALAR", TagNodeType::Scalar)]
    #[case("!include Mapping", TagNodeType::Mapping)]
    #[case("!include SEQUENCE", TagNodeType::Sequence)]
    fn parse_custom_tag_suffix_case_insensitive(
        #[case] input: &str,
        #[case] expected: TagNodeType,
    ) {
        let result = parse_custom_tag(input);
        assert_eq!(result.expected_type, Some(expected));
    }

    #[test]
    fn parse_custom_tag_double_space_before_suffix_not_recognized() {
        // Two spaces before suffix → not a recognized annotation, entire string is name.
        let result = parse_custom_tag("!include  scalar");
        assert_eq!(result.name, "!include  scalar");
        assert_eq!(result.expected_type, None);
    }

    #[test]
    fn parse_custom_tag_empty_string() {
        let result = parse_custom_tag("");
        assert_eq!(result.name, "");
        assert_eq!(result.expected_type, None);
    }

    #[test]
    fn parse_custom_tag_name_only_no_space() {
        let result = parse_custom_tag("!no-space-at-all");
        assert_eq!(result.name, "!no-space-at-all");
        assert_eq!(result.expected_type, None);
    }
}
