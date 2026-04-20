use super::*;
use rstest::rstest;

// -----------------------------------------------------------------------
// Group A: Verbatim tags on scalars
// -----------------------------------------------------------------------

#[test]
fn verbatim_tag_on_plain_scalar() {
    let events = evs("!<tag:yaml.org,2002:str> hello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello"
        )),
        "verbatim tag must be stored as URI content (without angle brackets)"
    );
}

#[test]
fn verbatim_tag_strips_angle_brackets() {
    let events = evs("!<my-uri> val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                ..
            } if t.as_ref() == "my-uri"
        )),
        "verbatim tag must store just 'my-uri', not '!<my-uri>'"
    );
}

#[test]
fn verbatim_tag_missing_closing_angle_bracket_returns_error() {
    assert!(
        has_error("!<tag:yaml.org,2002:str hello\n"),
        "verbatim tag missing '>' must return an error"
    );
}

#[test]
fn verbatim_tag_empty_uri_returns_error() {
    assert!(
        has_error("!<> val\n"),
        "empty verbatim tag URI must return an error"
    );
}

#[test]
fn verbatim_tag_whitespace_in_uri_returns_error() {
    // Space (0x20) is above the control-character threshold, but a tab
    // (0x09) is below 0x20 and must be rejected.
    assert!(
        has_error("!<foo\tbar> val\n"),
        "verbatim tag URI containing a tab must return an error"
    );
}

#[test]
fn verbatim_tag_control_char_in_uri_returns_error() {
    // NUL byte inside URI must be rejected.
    assert!(
        has_error("!<foo\x00bar> val\n"),
        "verbatim tag URI containing NUL must return an error"
    );
}

// -----------------------------------------------------------------------
// Group A2: Verbatim tag URI validation — YAML 1.2 §6.8.1 production [38]
// -----------------------------------------------------------------------

#[rstest]
#[case::alphanumeric("!<abc123> v\n")]
#[case::allowed_punctuation("!<a-_.~*'()[]#;/?:@&=+$,b> v\n")]
#[case::exclamation("!<tag:foo!bar> v\n")]
#[case::percent_encoded_space("!<%20> v\n")]
#[case::percent_encoded_slash("!<path%2Fto> v\n")]
fn verbatim_tag_uri_valid_chars_accepted(#[case] input: &str) {
    assert!(
        !has_error(input),
        "verbatim tag URI must be accepted: {input:?}"
    );
}

#[test]
fn verbatim_tag_uri_percent_uppercase_hex_accepted() {
    assert!(!has_error("!<%2F> v\n"), "uppercase %2F must be accepted");
    assert!(!has_error("!<%2f> v\n"), "lowercase %2f must be accepted");
}

#[rstest]
#[case::space("!<foo bar> v\n")]
#[case::curly_brace("!<foo{bar}> v\n")]
#[case::non_ascii("!<\u{4E2D}\u{6587}> v\n")]
#[case::bare_percent("!<%GG> v\n")]
#[case::percent_with_one_hex_digit("!<%2> v\n")]
#[case::del_char("!<foo\x7Fbar> v\n")]
#[case::vertical_bar("!<foo|bar> v\n")]
#[case::backslash("!<foo\\bar> v\n")]
#[case::less_than("!<foo<bar> v\n")]
fn verbatim_tag_uri_invalid_chars_rejected(#[case] input: &str) {
    assert!(
        has_error(input),
        "verbatim tag URI with invalid char must be rejected: {input:?}"
    );
}

#[test]
fn verbatim_tag_uri_embedded_close_delimiter_terminates_uri() {
    // First `>` closes the verbatim tag; `bar>` becomes part of the scalar value.
    let events = evs("!<foo>bar>\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "foo" && value.as_ref() == "bar>"
        )),
        "first '>' must close the verbatim tag URI; remainder is scalar content"
    );
}

// -----------------------------------------------------------------------
// Group B: Primary handle (`!!`) on scalars
// -----------------------------------------------------------------------

#[test]
fn primary_handle_on_plain_scalar() {
    // `!!str` expands to `"tag:yaml.org,2002:str"` via the default `!!` handle.
    let events = evs("!!str hello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello"
        )),
        "primary handle tag must expand to 'tag:yaml.org,2002:str'"
    );
}

#[test]
fn primary_handle_empty_suffix_expands_to_core_schema_prefix() {
    // `!! val` — primary handle with empty suffix; expands to `"tag:yaml.org,2002:"`.
    let events = evs("!! val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:"
        )),
        "primary handle with empty suffix must expand to 'tag:yaml.org,2002:'"
    );
}

// -----------------------------------------------------------------------
// Group C: Named handle (`!handle!suffix`)
// -----------------------------------------------------------------------

#[test]
fn named_handle_without_declaration_returns_error() {
    // `!e!tag val` — `!e!` handle is not declared via `%TAG`, so an error is expected.
    assert!(
        has_error("!e!tag val\n"),
        "named handle with no %TAG declaration must return an error"
    );
}

// -----------------------------------------------------------------------
// Group D: Secondary handle (`!suffix`)
// -----------------------------------------------------------------------

#[test]
fn secondary_handle_on_plain_scalar() {
    let events = evs("!yaml val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "!yaml" && value.as_ref() == "val"
        )),
        "secondary handle tag must be stored as '!yaml'"
    );
}

// -----------------------------------------------------------------------
// Group E: Non-specific tag (`!`)
// -----------------------------------------------------------------------

#[test]
fn non_specific_tag_on_plain_scalar() {
    // `! val` — bare `!` followed by space, then content.
    let events = evs("! val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "!" && value.as_ref() == "val"
        )),
        "non-specific tag '!' must be stored as '!'"
    );
}

// -----------------------------------------------------------------------
// Group F: Tags on collections (block)
// -----------------------------------------------------------------------

#[test]
fn tag_on_block_sequence() {
    let events = evs("!!seq\n- item\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                tag: Some(t),
                style: CollectionStyle::Block,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
        "block sequence must carry resolved tag 'tag:yaml.org,2002:seq'"
    );
}

#[test]
fn tag_on_block_mapping() {
    let events = evs("!!map\nkey: val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                tag: Some(t),
                style: CollectionStyle::Block,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:map"
        )),
        "block mapping must carry resolved tag 'tag:yaml.org,2002:map'"
    );
}

#[test]
fn tag_on_block_literal_scalar() {
    let events = evs("!!str |\n  hello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                style: ScalarStyle::Literal(Chomp::Clip),
                value,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello\n"
        )),
        "literal block scalar must carry resolved tag 'tag:yaml.org,2002:str'"
    );
}

#[test]
fn tag_on_block_folded_scalar() {
    let events = evs("!!str >\n  hello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                style: ScalarStyle::Folded(Chomp::Clip),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:str"
        )),
        "folded block scalar must carry resolved tag 'tag:yaml.org,2002:str'"
    );
}

// -----------------------------------------------------------------------
// Group G: Tags on collections (flow)
// -----------------------------------------------------------------------

#[test]
fn tag_on_flow_sequence() {
    let events = evs("!!seq [a, b]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                tag: Some(t),
                style: CollectionStyle::Flow,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
        "flow sequence must carry resolved tag 'tag:yaml.org,2002:seq'"
    );
}

#[test]
fn tag_on_flow_mapping() {
    let events = evs("!!map {a: b}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                tag: Some(t),
                style: CollectionStyle::Flow,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:map"
        )),
        "flow mapping must carry resolved tag 'tag:yaml.org,2002:map'"
    );
}

// -----------------------------------------------------------------------
// Group H: Tag + anchor combinations
// -----------------------------------------------------------------------

#[test]
fn tag_before_anchor_both_emitted_on_scalar() {
    let events = evs("!str &anchor value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                anchor: Some("anchor"),
                value,
                ..
            } if t.as_ref() == "!str" && value.as_ref() == "value"
        )),
        "tag before anchor: both must be emitted on the scalar"
    );
}

#[test]
fn anchor_before_tag_both_emitted_on_scalar() {
    let events = evs("&anchor !str value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                anchor: Some("anchor"),
                value,
                ..
            } if t.as_ref() == "!str" && value.as_ref() == "value"
        )),
        "anchor before tag: both must be emitted on the scalar"
    );
}

#[test]
fn tag_before_anchor_both_emitted_on_sequence() {
    let events = evs("!seq &s\n- item\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                tag: Some(t),
                anchor: Some("s"),
                ..
            } if t.as_ref() == "!seq"
        )),
        "tag before anchor on sequence: both must be emitted on SequenceStart"
    );
}

// -----------------------------------------------------------------------
// Group I: Error cases
// -----------------------------------------------------------------------

#[rstest]
#[case::duplicate_tag_on_same_node("!!str !!int val\n")]
#[case::alias_with_tag("&anchor val\n!!str *anchor\n")]
#[case::flow_duplicate_tag_on_same_node("[!t !t2 val]\n")]
#[case::flow_alias_with_pending_tag("[!t *a, val]\n")]
#[case::flow_alias_with_pending_tag_alone("[!a *name]\n")]
fn tag_error_cases_return_error(#[case] input: &str) {
    assert!(
        has_error(input),
        "invalid tag usage must return an error: {input:?}"
    );
}

#[test]
fn tag_length_at_limit_is_accepted() {
    // Verbatim tag with URI exactly MAX_TAG_LEN bytes long.
    let uri = "a".repeat(MAX_TAG_LEN);
    let input = format!("!<{uri}> val\n");
    assert!(
        !has_error(&input),
        "tag URI at exactly MAX_TAG_LEN bytes must be accepted"
    );
}

#[test]
fn tag_length_exceeding_limit_returns_error() {
    // Verbatim tag with URI one byte over MAX_TAG_LEN.
    let uri = "a".repeat(MAX_TAG_LEN + 1);
    let input = format!("!<{uri}> val\n");
    assert!(
        has_error(&input),
        "tag URI exceeding MAX_TAG_LEN bytes must return an error"
    );
}

#[test]
fn tag_with_invalid_char_stops_at_boundary() {
    // `!foo<bar val\n` — `<` is not a valid ns-tag-char per §6.8.1.
    // The tag must stop at `<`, yielding tag `!foo` and value `<bar val`.
    // (The parser does not error on this; it scans `!foo` as the tag
    // and treats the rest as the scalar value.)
    let events = evs("!foo<bar val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                ..
            } if t.as_ref() == "!foo"
        )),
        "tag scan must stop before '<' — tag must be '!foo'"
    );
}

#[test]
fn percent_encoded_tag_suffix_is_accepted() {
    // `!foo%2Fbar val\n` — `%2F` is a valid percent-encoded sequence.
    let events = evs("!foo%2Fbar val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                ..
            } if t.as_ref() == "!foo%2Fbar"
        )),
        "percent-encoded sequence in tag suffix must be accepted"
    );
}

#[test]
fn bare_percent_in_tag_stops_scan() {
    // Updated in Task 21: reference parser rejects this input per YAML 1.2
    // §6.8.1 (tag properties); conformance fix makes streaming parser match.
    // `!foo%zz\nhello\n` — `%zz` is not a valid percent-encoded sequence
    // (z is not a hex digit).  The tag scanner stops at `%`, yielding `!foo`
    // with `%zz` remaining inline (no space between tag and `%zz`).  Per YAML
    // 1.2 this is an invalid tag property (confirmed by rlsp-yaml-parser reference
    // impl which errors on this input).
    assert!(
        has_error("!foo%zz\nhello\n"),
        "tag followed immediately by bare '%' is a parse error"
    );
}

// -----------------------------------------------------------------------
// Group J: Span correctness
// -----------------------------------------------------------------------

#[test]
fn tagged_scalar_span_covers_value_not_tag() {
    // `!!str hello\n` — `!!str ` is 6 bytes; `hello` starts at byte 6.
    let items = parse_to_vec("!!str hello\n");
    let scalar_span = items.iter().find_map(|r| match r {
        Ok((Event::Scalar { tag: Some(_), .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(scalar_span.is_some(), "tagged scalar event must be present");
    if let Some(span) = scalar_span {
        assert_eq!(
            span.start.byte_offset, 6,
            "tagged scalar span must start at 'h' of 'hello' (byte 6), not at '!'"
        );
        assert_eq!(
            span.end.byte_offset, 11,
            "tagged scalar span must end after 'hello' (byte 11)"
        );
    }
}

#[test]
fn tagged_sequence_span_is_at_dash_indicator() {
    // `!!seq\n- a\n` — SequenceStart span should point to the `-` on line 2.
    let items = parse_to_vec("!!seq\n- a\n");
    let seq_span = items.iter().find_map(|r| match r {
        Ok((Event::SequenceStart { tag: Some(_), .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(
        seq_span.is_some(),
        "tagged SequenceStart event must be present"
    );
    // `!!seq\n` is 6 bytes; `-` is at byte 6.
    if let Some(span) = seq_span {
        assert_eq!(
            span.start.byte_offset, 6,
            "SequenceStart span must start at '-' indicator (byte 6), not at tag"
        );
    }
}

// -----------------------------------------------------------------------
// Group K: Regression — pre-existing silent drop
// -----------------------------------------------------------------------

#[test]
fn tag_prefix_line_not_silently_dropped() {
    // `!str value\n` was previously silently consumed by the fallback path
    // in `StepState::step` (`event_iter/step.rs`) that calls `consume_line`
    // for unrecognised content lines. This test ensures a Scalar event is
    // produced.
    let events = evs("!str value\n");
    assert!(
        events.iter().any(|e| matches!(e, Event::Scalar { .. })),
        "!str value must produce a Scalar event, not be silently dropped"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "!str" && value.as_ref() == "value"
        )),
        "scalar must have tag '!str' and value 'value'"
    );
}

#[test]
fn verbatim_tag_prefix_line_not_silently_dropped() {
    let events = evs("!<tag:yaml.org,2002:str> value\n");
    assert!(
        events.iter().any(|e| matches!(e, Event::Scalar { .. })),
        "verbatim-tagged value must produce a Scalar event"
    );
}

// -----------------------------------------------------------------------
// Group L: Tag on implicit mapping key context
// -----------------------------------------------------------------------

#[test]
fn tag_on_implicit_mapping_key_scalar() {
    // `!!str key: val\n` — tag is inline before the key, so it annotates
    // the key scalar, NOT the MappingStart (YAML test suite 9KAX).
    let events = evs("!!str key: val\n");
    // Key scalar carries the tag.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref().contains("str") && value.as_ref() == "key"
        )),
        "tag must be on key scalar, not on MappingStart"
    );
    // MappingStart has no tag.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { tag: None, .. })),
        "MappingStart must have no tag when tag is inline before key"
    );
}

// -----------------------------------------------------------------------
// Group M: Standalone tag applies to next node
// -----------------------------------------------------------------------

#[test]
fn standalone_tag_line_applies_to_scalar_below() {
    let events = evs("!!str\nhello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "hello"
        )),
        "standalone tag line must be attached to the following scalar"
    );
}

#[test]
fn standalone_tag_line_applies_to_sequence_below() {
    let events = evs("!!seq\n- a\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                tag: Some(t),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
        "standalone tag line must be attached to the following sequence"
    );
}

#[test]
fn standalone_tag_line_applies_to_mapping_below() {
    let events = evs("!!map\nkey: val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                tag: Some(t),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:map"
        )),
        "standalone tag line must be attached to the following mapping"
    );
}

// -----------------------------------------------------------------------
// Carry-forward note — Medium #2 (flow empty-element-with-tag)
//
// `[!!]` drops the tag silently; `[!, x]` returns "invalid leading comma".
// This mirrors the pre-existing Task 16 behaviour for anchors: `[&a]`
// drops the anchor and `[&a, x]` returns the same leading-comma error.
// Fixing flow empty-element handling for both anchors and tags requires
// deeper changes to the flow loop's has_value / emit logic and is tracked
// as a follow-up task (out of Task 17 scope).
// -----------------------------------------------------------------------

// -----------------------------------------------------------------------
// Group J: PendingTag enum consolidation (Task 16)
// -----------------------------------------------------------------------

// T-3: Standalone tag on block sequence — SequenceStart carries the tag; scalar has none.
#[test]
fn standalone_tag_on_block_sequence_propagates_to_sequence_start() {
    let events = evs("!!seq\n- a\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
        "SequenceStart must carry tag:yaml.org,2002:seq"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: None, value, .. } if value.as_ref() == "a"
        )),
        "sequence item scalar must have no tag"
    );
}

// T-4: Inline tag on mapping key — MappingStart has no tag; key scalar carries the tag.
#[test]
fn inline_tag_on_mapping_key_annotates_key_scalar_not_mapping() {
    let events = evs("!!str key: value\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { tag: None, .. })),
        "MappingStart must have no tag when !!str is inline before key"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "key"
        )),
        "key scalar must carry tag:yaml.org,2002:str"
    );
}

// T-5: Standalone tag on block mapping — MappingStart carries the tag; key scalar has none.
#[test]
fn standalone_tag_on_block_mapping_propagates_to_mapping_start() {
    let events = evs("!!map\nkey: value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:map"
        )),
        "MappingStart must carry tag:yaml.org,2002:map"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: None, value, .. } if value.as_ref() == "key"
        )),
        "key scalar must have no tag"
    );
}

// T-6: Verbatim tag passes through unchanged.
#[test]
fn verbatim_tag_passes_through_unchanged() {
    let events = evs("!<tag:example.com/foo> value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:example.com/foo" && value.as_ref() == "value"
        )),
        "verbatim tag must be preserved as-is on the scalar"
    );
}

// T-7: Local tag preserved as-is.
#[test]
fn local_tag_preserved_as_is() {
    let events = evs("!local value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "!local" && value.as_ref() == "value"
        )),
        "local tag !local must be preserved unchanged"
    );
}

// T-8: Tag resolved via %TAG directive — Cow::Owned flows through enum variant correctly.
#[test]
fn tag_resolved_via_pct_tag_directive_cow_owned() {
    let input = "%TAG !custom! tag:example.com/\n---\n!custom!foo value\n";
    let events = evs(input);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:example.com/foo" && value.as_ref() == "value"
        )),
        "resolved tag tag:example.com/foo must flow through PendingTag::Inline correctly"
    );
}

// T-9: Tag cleared after use — second sequence item has no tag.
#[test]
fn tag_cleared_after_use_second_item_has_none() {
    let events = evs("- !!str first\n- second\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), value, .. }
                if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "first"
        )),
        "first item must carry tag:yaml.org,2002:str"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: None, value, .. } if value.as_ref() == "second"
        )),
        "second item must have no tag"
    );
}

// T-10: Tag on flow sequence — SequenceStart carries the tag with Flow style.
#[test]
fn tag_on_flow_sequence_propagates_to_sequence_start() {
    let events = evs("!!seq [a, b]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                tag: Some(t),
                style: CollectionStyle::Flow,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
        "SequenceStart for flow sequence must carry tag:yaml.org,2002:seq"
    );
}

// T-11: Tag + anchor both on standalone collection — both propagate through distinct enums.
#[test]
fn tag_and_anchor_both_standalone_both_propagate_to_sequence_start() {
    let input = "&myseq\n!!seq\n- a\n";
    let events = evs(input);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("myseq"),
                tag: Some(t),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
        "SequenceStart must carry both anchor myseq and tag:yaml.org,2002:seq"
    );
}

// T-12: Inline tag + inline anchor on same scalar — both attached to the scalar.
#[test]
fn inline_tag_and_anchor_on_same_scalar_both_attached() {
    let events = evs("!!str &a value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                tag: Some(t),
                anchor: Some("a"),
                value,
                ..
            } if t.as_ref() == "tag:yaml.org,2002:str" && value.as_ref() == "value"
        )),
        "scalar must carry both tag:yaml.org,2002:str and anchor a"
    );
}

// -----------------------------------------------------------------------
// Group TL: tag_loc span correctness
// -----------------------------------------------------------------------

// TL-1: Inline shorthand tag on scalar — tag_loc covers from `!` through last byte of token.
#[test]
fn tag_loc_inline_shorthand_on_scalar() {
    // `!!str hello\n` — `!!str` is 5 bytes at byte 0, col 0, line 1.
    let items = parse_to_vec("!!str hello\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "inline tag on scalar must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 1,
                column: 5,
            },
        };
        assert_eq!(loc, expected, "tag_loc must cover `!!str` (5 bytes)");
    }
}

// TL-2: Verbatim tag on scalar — tag_loc covers `!<URI>` including angle brackets.
#[test]
fn tag_loc_verbatim_tag_on_scalar() {
    // `!<tag:yaml.org,2002:str> hello\n` — 24 bytes: `!` + `<tag:yaml.org,2002:str>`.
    // `<tag:yaml.org,2002:str>` = 23 chars, total token = 24 bytes.
    let items = parse_to_vec("!<tag:yaml.org,2002:str> hello\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "verbatim tag on scalar must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        assert_eq!(loc.start.byte_offset, 0, "tag_loc must start at `!`");
        assert_eq!(
            loc.end.byte_offset, 24,
            "tag_loc must end after `>` of verbatim tag (24 bytes)"
        );
    }
}

// TL-3: Standalone tag on scalar — tag_loc points to tag token on its own line.
#[test]
fn tag_loc_standalone_tag_on_scalar() {
    // `!!str\nhello\n` — `!!str` at byte 0, col 0, line 1; 5 bytes.
    let items = parse_to_vec("!!str\nhello\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "standalone tag on scalar must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 1,
                column: 5,
            },
        };
        assert_eq!(
            loc, expected,
            "tag_loc for standalone `!!str` must cover 5 bytes"
        );
    }
}

// TL-4: Standalone tag on block sequence — SequenceStart carries tag_loc.
#[test]
fn tag_loc_standalone_tag_on_block_sequence() {
    // `!!seq\n- a\n` — `!!seq` at byte 0, 5 bytes.
    let items = parse_to_vec("!!seq\n- a\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::SequenceStart {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "tagged SequenceStart must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 1,
                column: 5,
            },
        };
        assert_eq!(
            loc, expected,
            "SequenceStart tag_loc must cover `!!seq` (5 bytes)"
        );
    }
}

// TL-5: Standalone tag on block mapping — MappingStart carries tag_loc.
#[test]
fn tag_loc_standalone_tag_on_block_mapping() {
    // `!!map\nkey: val\n` — `!!map` at byte 0, 5 bytes.
    let items = parse_to_vec("!!map\nkey: val\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::MappingStart {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "tagged MappingStart must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 1,
                column: 5,
            },
        };
        assert_eq!(
            loc, expected,
            "MappingStart tag_loc must cover `!!map` (5 bytes)"
        );
    }
}

// TL-6: Inline tag on flow sequence — SequenceStart carries tag_loc.
#[test]
fn tag_loc_inline_tag_on_flow_sequence() {
    // `!!seq [a, b]\n` — `!!seq` at byte 0, 5 bytes.
    let items = parse_to_vec("!!seq [a, b]\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::SequenceStart {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "flow SequenceStart with tag must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 1,
                column: 5,
            },
        };
        assert_eq!(
            loc, expected,
            "flow SequenceStart tag_loc must cover `!!seq` (5 bytes)"
        );
    }
}

// TL-7: Inline tag on flow mapping — MappingStart carries tag_loc.
#[test]
fn tag_loc_inline_tag_on_flow_mapping() {
    // `!!map {k: v}\n` — `!!map` at byte 0, 5 bytes.
    let items = parse_to_vec("!!map {k: v}\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::MappingStart {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "flow MappingStart with tag must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 5,
                line: 1,
                column: 5,
            },
        };
        assert_eq!(
            loc, expected,
            "flow MappingStart tag_loc must cover `!!map` (5 bytes)"
        );
    }
}

// TL-8: Inline tag on implicit mapping key scalar — key scalar carries tag_loc.
#[test]
fn tag_loc_inline_tag_on_mapping_key_scalar() {
    // `!!str key: val\n` — tag is inline before the key; tag_loc on key scalar.
    let items = parse_to_vec("!!str key: val\n");
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "key scalar with inline tag must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        assert_eq!(loc.start.byte_offset, 0, "tag_loc must start at `!`");
        assert_eq!(
            loc.end.byte_offset, 5,
            "tag_loc must end after `!!str` (5 bytes)"
        );
    }
}

// TL-4: Named handle via %TAG directive — tag_loc covers the token on the content line.
#[test]
fn tag_loc_named_handle_with_pct_tag_directive() {
    // `%TAG !h! tag:ex,2026:\n---\n!h!s 42\n`
    // Line 1: `%TAG !h! tag:ex,2026:\n` = 22 bytes (bytes 0-21)
    // Line 2: `---\n`                   = 4 bytes  (bytes 22-25)
    // Line 3: `!h!s 42\n`              starts at byte 26
    // `!h!s` = 4 bytes → tag_loc = bytes 26..30
    let input = "%TAG !h! tag:ex,2026:\n---\n!h!s 42\n";
    let items = parse_to_vec(input);
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                tag: Some(t),
                tag_loc: Some(s),
                ..
            } = ev
            {
                if t.as_ref() == "tag:ex,2026:s" {
                    Some(*s)
                } else {
                    None
                }
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "named-handle scalar must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        assert_eq!(
            loc.start.byte_offset, 26,
            "tag_loc must start at `!` of `!h!s` (byte 26)"
        );
        assert_eq!(
            loc.end.byte_offset, 30,
            "tag_loc must end after `!h!s` (4 bytes, byte 30)"
        );
    }
}

// TL-9 / TL-11: No tag — tag_loc is None across all node types.
#[rstest]
#[case::plain_scalar("plain value\n")]
#[case::block_sequence("- item\n")]
#[case::block_mapping("k: v\n")]
#[case::flow_sequence("[1, 2]\n")]
#[case::flow_mapping("{k: v}\n")]
fn tag_loc_none_when_no_tag(#[case] input: &str) {
    let items = parse_to_vec(input);
    for (ev, _) in items.iter().filter_map(|r| r.as_ref().ok()) {
        let tag_loc_val = match ev {
            Event::Scalar { tag_loc, .. }
            | Event::SequenceStart { tag_loc, .. }
            | Event::MappingStart { tag_loc, .. } => Some(tag_loc),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::Alias { .. }
            | Event::SequenceEnd
            | Event::MappingEnd => None,
        };
        if let Some(loc) = tag_loc_val {
            assert!(
                loc.is_none(),
                "untagged node must have tag_loc: None for input {input:?}, got {loc:?}"
            );
        }
    }
}

// TL-10: tag_loc byte_offset is correct when multi-byte UTF-8 precedes the tag on the line.
#[test]
fn tag_loc_utf8_scalar_key_tag_byte_offset() {
    // `é: !!str val\n` — `é` (U+00E9) is 2 UTF-8 bytes; `: ` is 2 bytes.
    // `!!str` starts at byte 4, not char-index 3.
    // tag_loc.start.byte_offset must be 4 (bytes), not 3 (chars).
    let input = "\u{00e9}: !!str val\n";
    let items = parse_to_vec(input);
    let loc_opt = items.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                tag: Some(_),
                tag_loc: Some(s),
                ..
            } = ev
            {
                Some(*s)
            } else {
                None
            }
        })
    });
    assert!(
        loc_opt.is_some(),
        "tagged scalar after UTF-8 key must have tag_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        assert_eq!(
            loc.start.byte_offset, 4,
            "tag_loc.start.byte_offset must be 4 (bytes), not 3 (chars) — é is 2 UTF-8 bytes"
        );
        assert_eq!(
            loc.end.byte_offset, 9,
            "tag_loc.end.byte_offset must be 9 (byte 4 + 5 bytes for `!!str`)"
        );
    }
}

// TL-10: Invariant — tag.is_some() == tag_loc.is_some() for every event.
#[test]
fn tag_loc_invariant_tag_some_iff_loc_some() {
    let input = "!!str hello\n!!seq\n- a\n!!map\nk: v\n";
    let events = parse_to_vec(input);
    for (ev, _) in events.iter().filter_map(|r| r.as_ref().ok()) {
        let pair = match ev {
            Event::Scalar { tag, tag_loc, .. }
            | Event::SequenceStart { tag, tag_loc, .. }
            | Event::MappingStart { tag, tag_loc, .. } => Some((tag.is_some(), tag_loc.is_some())),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::Alias { .. }
            | Event::SequenceEnd
            | Event::MappingEnd => None,
        };
        if let Some((tag_is_some, tag_loc_is_some)) = pair {
            assert_eq!(
                tag_is_some, tag_loc_is_some,
                "invariant violated: tag.is_some()={tag_is_some} but tag_loc.is_some()={tag_loc_is_some} for event {ev:?}"
            );
        }
    }
}

// TL-11: Two tagged scalars in same document — each gets its own tag_loc.
#[test]
fn tag_loc_two_tagged_scalars_each_gets_own_loc() {
    // `!!str hello\n!!int 42\n` (two documents since standalone tags + scalars)
    // Use a mapping to keep both in one document: `a: !!str hello\nb: !!int 42\n`
    let input = "a: !!str hello\nb: !!int 42\n";
    let items = parse_to_vec(input);
    let locs: Vec<Span> = items
        .iter()
        .filter_map(|r| {
            r.as_ref().ok().and_then(|(ev, _)| {
                if let Event::Scalar {
                    tag: Some(_),
                    tag_loc: Some(s),
                    ..
                } = ev
                {
                    Some(*s)
                } else {
                    None
                }
            })
        })
        .collect();
    // `!!str` starts after `a: ` = byte 3; `!!int` starts after `b: ` on line 2.
    // Line 1: `a: !!str hello\n` = 15 bytes. Line 2 starts at byte 15: `b: !!int 42\n`.
    // `!!str` at byte 3; `!!int` at byte 15 + 3 = 18.
    assert_eq!(locs.len(), 2, "must find exactly two tagged scalars");
    if let [first, second] = locs.as_slice() {
        assert_eq!(
            first.start.byte_offset, 3,
            "first tagged scalar tag_loc starts at byte 3 (`!!str`)"
        );
        assert_eq!(
            first.end.byte_offset, 8,
            "first tag_loc ends at byte 8 (after `!!str`)"
        );
        assert_eq!(
            second.start.byte_offset, 18,
            "second tagged scalar tag_loc starts at byte 18 (`!!int`)"
        );
        assert_eq!(
            second.end.byte_offset, 23,
            "second tag_loc ends at byte 23 (after `!!int`)"
        );
    }
}

// TL-CORPUS: tag.is_some() == tag_loc.is_some() invariant holds across yaml-test-suite.
#[test]
fn tag_loc_invariant_corpus_wide() {
    let suite_dir =
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/yaml-test-suite/src");
    let read_result = std::fs::read_dir(&suite_dir);
    assert!(
        read_result.is_ok(),
        "cannot read yaml-test-suite dir {suite_dir:?}"
    );
    let mut file_count = 0u32;
    for entry in read_result.into_iter().flatten().flatten() {
        let path = entry.path();
        if path.extension().and_then(|s| s.to_str()) != Some("yaml") {
            continue;
        }
        file_count += 1;
        let read_file = std::fs::read_to_string(&path);
        assert!(read_file.is_ok(), "cannot read {path:?}");
        let content = read_file.unwrap_or_default();
        let file_name = path.file_name().unwrap_or_default().to_string_lossy();
        for (ev, _) in rlsp_yaml_parser::parse_events(&content).filter_map(Result::ok) {
            let pair = match &ev {
                Event::Scalar { tag, tag_loc, .. }
                | Event::SequenceStart { tag, tag_loc, .. }
                | Event::MappingStart { tag, tag_loc, .. } => {
                    Some((tag.is_some(), tag_loc.is_some()))
                }
                Event::StreamStart
                | Event::StreamEnd
                | Event::Comment { .. }
                | Event::DocumentStart { .. }
                | Event::DocumentEnd { .. }
                | Event::Alias { .. }
                | Event::SequenceEnd
                | Event::MappingEnd => None,
            };
            if let Some((tag_is_some, tag_loc_is_some)) = pair {
                assert_eq!(
                    tag_is_some, tag_loc_is_some,
                    "invariant violated in {file_name}: tag.is_some()={tag_is_some} but tag_loc.is_some()={tag_loc_is_some} for event {ev:?}"
                );
            }
        }
    }
    assert!(file_count > 0, "no .yaml files found in {suite_dir:?}");
}
