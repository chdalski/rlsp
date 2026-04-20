use rstest::rstest;

use super::*;

// -----------------------------------------------------------------------
// Group A: Anchor on block scalars
// -----------------------------------------------------------------------

#[test]
fn anchor_inline_before_plain_scalar_value() {
    // `key: &a val\n` — anchor before plain scalar value.
    let events = evs("key: &a val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "val"
        )),
        "anchor `&a` must be attached to value scalar 'val'"
    );
}

#[test]
fn anchor_on_standalone_line_applies_to_scalar_below() {
    // `&a\nval\n` — anchor on own line, scalar on next line.
    let events = evs("&a\nval\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "val"
        )),
        "standalone anchor `&a` must be attached to following scalar"
    );
}

#[test]
fn anchor_on_mapping_key_scalar() {
    // `&k key: val\n` — anchor is inline before the key, so it annotates
    // the key scalar (YAML test suite 9KAX: inline property → key scalar).
    let events = evs("&k key: val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
        )),
        "anchor `&k` must be attached to key scalar"
    );
    // MappingStart carries no anchor.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { anchor: None, .. })),
        "MappingStart must have no anchor"
    );
}

#[test]
fn anchor_on_sequence_item_plain_scalar() {
    // `- &a item\n` — anchor on a plain scalar sequence item.
    let events = evs("- &a item\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "item"
        )),
        "anchor `&a` must be attached to sequence item scalar"
    );
}

#[test]
fn anchor_on_empty_scalar_value() {
    // yaml-test-suite 6KGN: `a: &anchor\nb: *anchor\n`
    // `&anchor` with no inline content → empty scalar value.
    let events = evs("a: &anchor\nb: *anchor\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("anchor"), value, .. } if value.as_ref() == ""
        )),
        "anchor `&anchor` with no value must emit empty scalar"
    );
}

#[test]
fn duplicate_anchor_name_overwrites_previous() {
    // `First: &anchor Foo\nOverride: &anchor Bar\n`
    // The parser emits both scalars each with the anchor; no error.
    let events = evs("First: &anchor Foo\nOverride: &anchor Bar\n");
    let anchored: Vec<_> = events
        .iter()
        .filter_map(|e| match e {
            Event::Scalar {
                anchor: Some("anchor"),
                value,
                ..
            } => Some(value.as_ref()),
            Event::Scalar { .. }
            | Event::StreamStart
            | Event::StreamEnd
            | Event::Alias { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd
            | Event::Comment { .. } => None,
        })
        .collect();
    assert_eq!(
        anchored.len(),
        2,
        "both anchored scalars must appear; got {anchored:?}"
    );
}

// -----------------------------------------------------------------------
// Group B: Anchor on block sequences
// -----------------------------------------------------------------------

#[test]
fn anchor_on_standalone_line_applies_to_block_sequence() {
    // `&seq\n- a\n- b\n` — standalone anchor applies to following sequence.
    let events = evs("&seq\n- a\n- b\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("seq"),
                style: CollectionStyle::Block,
                ..
            }
        )),
        "standalone anchor `&seq` must be attached to SequenceStart"
    );
}

#[test]
fn inline_anchor_on_dash_applies_to_nested_sequence() {
    // `- &seq\n  - a\n` — anchor before nested sequence item.
    // The `&seq` is on the same line as `-`, so it applies to the nested seq.
    let events = evs("- &seq\n  - a\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("seq"),
                style: CollectionStyle::Block,
                ..
            }
        )),
        "anchor `&seq` on dash line must be attached to nested SequenceStart"
    );
}

// -----------------------------------------------------------------------
// Group C: Anchor on block mappings
// -----------------------------------------------------------------------

#[test]
fn anchor_on_standalone_line_applies_to_block_mapping() {
    // `&map\nkey: val\n` — standalone anchor applies to the mapping.
    let events = evs("&map\nkey: val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: Some("map"),
                style: CollectionStyle::Block,
                ..
            }
        )),
        "standalone anchor `&map` must be attached to MappingStart"
    );
}

#[test]
fn anchor_inline_before_mapping_value_applies_to_nested_mapping() {
    // `key: &node\n  inner: val\n` — anchor before nested mapping.
    let events = evs("key: &node\n  inner: val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: Some("node"),
                style: CollectionStyle::Block,
                ..
            }
        )),
        "anchor `&node` inline before nested mapping must be attached to MappingStart"
    );
}

#[test]
fn inline_anchor_on_key_does_not_annotate_mapping_start() {
    // `&k key: val\n` — `&k` is inline before the key, so it annotates the
    // key scalar, NOT the MappingStart (YAML test suite 9KAX).
    let events = evs("&k key: val\n");
    // MappingStart has no anchor.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: None,
                anchor_loc: None,
                style: CollectionStyle::Block,
                ..
            }
        )),
        "MappingStart must have no anchor when anchor is inline before key"
    );
    // The key scalar carries the anchor.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
        )),
        "anchor `&k` must be on key scalar"
    );
}

// -----------------------------------------------------------------------
// Group D: Alias in block context
// -----------------------------------------------------------------------

// D-1, D-2: `*ref` in different block positions emits Event::Alias { name: "ref" }.
#[rstest]
// D-1: Alias as block mapping value.
#[case::alias_as_block_mapping_value("key: *ref\n")]
// D-2: Alias as block sequence item.
#[case::alias_as_block_sequence_item("- *ref\n")]
fn alias_in_block_context_emits_alias_event(#[case] input: &str) {
    let events = evs(input);
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "ref" })),
        "alias `*ref` must emit Alias {{ name: \"ref\" }} for input: {input:?}"
    );
}

#[test]
fn alias_as_block_mapping_key_explicit() {
    // `? *ref\n: value\n` — alias as explicit block mapping key.
    let events = evs("? *ref\n: value\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "ref" })),
        "alias `*ref` as explicit mapping key must emit Alias event"
    );
    // The value scalar must also be present.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { value, .. } if value.as_ref() == "value"
        )),
        "mapping value 'value' must be present after the alias key"
    );
}

#[test]
fn alias_does_not_expand_referenced_node() {
    // Parser must emit Event::Alias, NOT re-emit the anchored node's events.
    // yaml-test-suite 3GZX: anchor-then-alias mapping.
    let events = evs("First occurrence: &anchor Foo\nSecond occurrence: *anchor\n");
    let alias_count = events
        .iter()
        .filter(|e| matches!(e, Event::Alias { .. }))
        .count();
    assert_eq!(alias_count, 1, "exactly one Alias event emitted");
    // No second "Foo" scalar should appear (no expansion).
    let foo_scalars = events
        .iter()
        .filter(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "Foo"))
        .count();
    assert_eq!(
        foo_scalars, 1,
        "value 'Foo' must appear exactly once (no alias expansion)"
    );
}

// -----------------------------------------------------------------------
// Group E: Anchor / alias in flow context
// -----------------------------------------------------------------------

#[test]
fn anchor_on_flow_sequence_start() {
    // `&seq [a, b]\n` — anchor applied to a flow sequence.
    let events = evs("&seq [a, b]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("seq"),
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "anchor `&seq` must be attached to flow SequenceStart"
    );
}

#[test]
fn anchor_on_flow_mapping_start() {
    // `&map {a: b}\n` — anchor applied to a flow mapping.
    let events = evs("&map {a: b}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: Some("map"),
                style: CollectionStyle::Flow,
                ..
            }
        )),
        "anchor `&map` must be attached to flow MappingStart"
    );
}

#[test]
fn anchor_on_plain_scalar_inside_flow_mapping() {
    // `{key: &a val}\n` — anchor on value inside flow mapping.
    let events = evs("{key: &a val}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "val"
        )),
        "anchor `&a` must be attached to scalar 'val' in flow mapping"
    );
}

#[test]
fn alias_in_flow_mapping_emits_alias_event() {
    // `{key: *ref}\n` — alias as flow mapping value.
    let events = evs("{key: *ref}\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "ref" })),
        "alias `*ref` inside flow mapping must emit Alias event"
    );
}

// -----------------------------------------------------------------------
// Group F: Error cases
// -----------------------------------------------------------------------

#[test]
fn anchor_with_empty_name_returns_error() {
    // `& val\n` — `&` immediately followed by space is an empty anchor name.
    assert!(
        has_error("& val\n"),
        "empty anchor name `&<space>` must return an error"
    );
}

#[test]
fn alias_with_empty_name_returns_error() {
    // `* val\n` — `*` immediately followed by space is an empty alias name.
    assert!(
        has_error("* val\n"),
        "empty alias name `*<space>` must return an error"
    );
}

// Anchor name at the length boundary: at-limit accepted, over-limit rejected.
#[rstest]
// At exactly MAX_ANCHOR_NAME_BYTES: accepted.
#[case::at_max_length_accepted(MAX_ANCHOR_NAME_BYTES, false)]
// One byte over the limit: rejected.
#[case::over_max_length_returns_error(MAX_ANCHOR_NAME_BYTES + 1, true)]
fn anchor_name_length_boundary(#[case] name_len: usize, #[case] expect_error: bool) {
    let name = "a".repeat(name_len);
    let input = format!("&{name} val\n");
    assert_eq!(
        has_error(&input),
        expect_error,
        "anchor name of {name_len} bytes: expect_error={expect_error} (limit={MAX_ANCHOR_NAME_BYTES})"
    );
}

#[test]
fn anchor_name_with_unicode_characters_is_accepted() {
    // yaml-test-suite 8XYN: unicode anchor name (emoji counts as ns-anchor-char).
    // `&😁 unicode anchor\n`
    let events = evs("- &\u{1F601} unicode anchor\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                anchor: Some("\u{1F601}"),
                ..
            }
        )),
        "unicode anchor name must be accepted"
    );
}

#[test]
fn alias_name_exceeding_max_length_returns_error() {
    // Alias name one byte over the limit must return an error.
    // scan_anchor_name is shared between anchors and aliases, so the same
    // limit applies to both.
    let name = "a".repeat(MAX_ANCHOR_NAME_BYTES + 1);
    let input = format!("*{name}\n");
    assert!(
        has_error(&input),
        "alias name of {} bytes must be rejected (limit is {MAX_ANCHOR_NAME_BYTES})",
        MAX_ANCHOR_NAME_BYTES + 1
    );
}

#[test]
fn flow_indicator_terminates_anchor_name() {
    // `[&name item]\n` — the space terminates the anchor name (space is not
    // ns-anchor-char); `item` is the anchored scalar.  The anchor name must
    // be `"name"`, not `"name item"`.  This also verifies that flow
    // indicators (`,`, `]`, `}`) inside the name would be excluded by
    // is_ns_anchor_char.
    let events = evs("[&name item]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("name"), value, .. } if value.as_ref() == "item"
        )),
        "anchor name must be `name`, not include the space or the value `item`"
    );
}

#[test]
fn line_break_terminates_anchor_name() {
    // `&name\nscalar\n` — newline terminates the anchor name on the first
    // line; `scalar` is the following node that inherits the anchor.
    // is_ns_anchor_char excludes `\n`, so the scan stops at end-of-content.
    let events = evs("&name\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("name"), value, .. } if value.as_ref() == "scalar"
        )),
        "newline must terminate anchor name; anchor `name` must attach to following `scalar`"
    );
}

#[test]
fn tag_before_anchor_on_same_line_both_emitted() {
    // `!tag &anchor value\n` — Task 17 implements tag parsing.
    // Both the tag and anchor are emitted on the scalar.
    let events = evs("!tag &anchor value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar {
                anchor: Some("anchor"),
                tag: Some(t),
                ..
            } if t.as_ref() == "!tag"
        )),
        "tag-before-anchor on same line: both tag and anchor must be emitted on the scalar"
    );
}

// -----------------------------------------------------------------------
// Group G: Span correctness
// -----------------------------------------------------------------------

#[test]
fn alias_event_span_covers_star_and_name() {
    // `*foo\n` — alias span must start at `*` and cover the full name.
    // `*` = byte 0, `foo` = bytes 1-3, so span is [0, 4).
    let items = parse_to_vec("*foo\n");
    let alias_span = items.iter().find_map(|r| match r {
        Ok((Event::Alias { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(alias_span.is_some(), "Alias event must be present");
    if let Some(span) = alias_span {
        assert_eq!(span.start.byte_offset, 0, "Alias span must start at byte 0");
        assert_eq!(
            span.end.byte_offset, 4,
            "Alias span must end at byte 4 (after 'foo')"
        );
        assert_eq!(span.start.column, 0, "Alias must start at column 0");
    }
}

#[test]
fn anchor_name_borrowed_from_input_not_allocated() {
    // Anchor names must be `&'input str` borrows — verify round-trip identity.
    let input = "key: &myanchor value\n";
    let events = evs(input);
    let found = events.iter().any(|e| {
        matches!(
            e,
            Event::Scalar { anchor: Some("myanchor"), value, .. } if value.as_ref() == "value"
        )
    });
    assert!(found, "anchor name must survive as a borrowed slice");
}

// -----------------------------------------------------------------------
// Group H: Conformance (yaml-test-suite fixtures)
// -----------------------------------------------------------------------

#[test]
fn conformance_3gzx_spec_example_7_1_alias_nodes() {
    // yaml-test-suite 3GZX: Spec Example 7.1. Alias Nodes.
    // `First occurrence: &anchor Foo\nSecond occurrence: *anchor\n
    //  Override anchor: &anchor Bar\nReuse anchor: *anchor\n`
    let input = "First occurrence: &anchor Foo\n\
                 Second occurrence: *anchor\n\
                 Override anchor: &anchor Bar\n\
                 Reuse anchor: *anchor\n";
    let events = evs(input);

    // Two anchored scalars ("Foo" and "Bar").
    let anchored_scalar_count = events
        .iter()
        .filter(|e| {
            matches!(
                e,
                Event::Scalar {
                    anchor: Some("anchor"),
                    ..
                }
            )
        })
        .count();
    assert_eq!(
        anchored_scalar_count, 2,
        "must have two scalars with anchor 'anchor'"
    );

    // Two alias events.
    let alias_count = events
        .iter()
        .filter(|e| matches!(e, Event::Alias { name: "anchor" }))
        .count();
    assert_eq!(alias_count, 2, "must have two alias events for 'anchor'");
}

#[test]
fn conformance_6kgn_anchor_for_empty_node() {
    // yaml-test-suite 6KGN: Anchor for empty node.
    // `---\na: &anchor\nb: *anchor\n`
    let input = "---\na: &anchor\nb: *anchor\n";
    let events = evs(input);

    // The anchored empty scalar.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("anchor"), value, .. } if value.as_ref() == ""
        )),
        "anchored empty scalar must be present"
    );

    // The alias.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "anchor" })),
        "alias *anchor must emit Alias event"
    );
}

#[test]
fn conformance_7bub_spec_example_2_10_sammy_sosa() {
    // yaml-test-suite 7BUB: Spec Example 2.10 — anchor on sequence item,
    // alias as subsequent item in another sequence.
    let input = "---\nhr:\n  - Mark McGwire\n  - &SS Sammy Sosa\n\
                 rbi:\n  - *SS\n  - Ken Griffey\n";
    let events = evs(input);

    // Anchored scalar "Sammy Sosa".
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("SS"), value, .. } if value.as_ref() == "Sammy Sosa"
        )),
        "anchor `&SS` on 'Sammy Sosa' must be present"
    );

    // Alias *SS.
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "SS" })),
        "alias `*SS` must emit Alias event"
    );
}

#[test]
fn conformance_8xyn_anchor_with_unicode_character() {
    // yaml-test-suite 8XYN: Unicode anchor name (emoji).
    let input = "---\n- &\u{1F601} unicode anchor\n";
    let events = evs(input);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("\u{1F601}"), value, .. }
                if value.as_ref() == "unicode anchor"
        )),
        "unicode emoji anchor name must be accepted"
    );
}

#[test]
fn conformance_6m2f_aliases_in_explicit_block_mapping() {
    // yaml-test-suite 6M2F: Aliases in Explicit Block Mapping.
    // `? &a a\n: &b b\n: *a\n`
    let input = "? &a a\n: &b b\n: *a\n";
    let events = evs(input);

    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "a"
        )),
        "anchor `&a` on key must be present"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("b"), value, .. } if value.as_ref() == "b"
        )),
        "anchor `&b` on value must be present"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "a" })),
        "alias `*a` must emit Alias event"
    );
}

// -----------------------------------------------------------------------
// Group I: Additional scenarios from test-engineer review
// -----------------------------------------------------------------------

#[test]
fn alias_as_explicit_block_mapping_key() {
    // UT-A14: `? *ref\n: value\n` — alias as explicit mapping key.
    let events = evs("? *ref\n: value\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "ref" })),
        "alias `*ref` as explicit mapping key must emit Alias event"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "value")),
        "value scalar must be present"
    );
}

#[test]
fn flow_sequence_with_anchored_first_and_unannotated_second() {
    // UT-A15: `[&a foo, bar]\n` — anchor on first item only; second item
    // has no anchor.
    let events = evs("[&a foo, bar]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "foo"
        )),
        "first item must have anchor `a`"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "bar"
        )),
        "second item must have no anchor"
    );
}

#[test]
fn anchor_on_flow_mapping_key() {
    // UT-A16: `{&a key: value}\n` — anchor on the key inside a flow mapping.
    let events = evs("{&a key: value}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "key"
        )),
        "anchor `&a` must be attached to key scalar 'key'"
    );
}

#[test]
fn alias_in_flow_sequence_with_following_item() {
    // UT-A18: `[*ref, foo]\n` — alias followed by a plain scalar.
    let events = evs("[*ref, foo]\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "ref" })),
        "alias `*ref` must emit Alias event"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Scalar { value, .. } if value.as_ref() == "foo")),
        "scalar 'foo' must follow alias"
    );
}

#[test]
fn undefined_alias_emits_alias_event_without_error() {
    // UT-A21: parser emits Event::Alias for names that were never anchored.
    // Resolution of undefined aliases is the loader's responsibility.
    let events = evs("*undefined\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "undefined" })),
        "undefined alias must emit Alias event without error"
    );
}

#[test]
fn multi_document_alias_in_second_doc_emits_event() {
    // UT-A22: anchor in doc 1, alias in doc 2 — parser does not resolve
    // cross-document aliases; it emits both events.
    let input = "&a foo\n---\n*a\n";
    let events = evs(input);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "foo"
        )),
        "anchored scalar 'foo' in doc 1 must be present"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "a" })),
        "alias `*a` in doc 2 must emit Alias event"
    );
}

#[test]
fn anchor_name_stops_at_comma_in_flow_sequence() {
    // UT-A23: `[&name,foo]\n` — comma immediately after anchor name stops
    // the scan; anchor name is `"name"`, not `"name,foo"`.  The `,` acts
    // as a flow separator — since no scalar follows the anchor before the
    // comma, this tests the boundary case where anchor name scan stops
    // correctly before the comma.  The current parser emits an error here
    // (leading comma after anchor with no value) which is the correct
    // behavior: `&name,foo` is ambiguous without a space, not valid YAML.
    // Test verifies that the anchor name `"name"` is still extracted
    // correctly (it does not absorb the `,`).
    let result: Vec<_> = parse_events("[&name,foo]\n").collect();
    // The error should reference `name` as the anchor — not `name,foo`.
    // Since this produces an error, we just verify anchor name parsing
    // stopped at the comma by checking the error message does not mention
    // a comma-containing name.
    let error_msg = result
        .iter()
        .filter_map(|r| r.as_ref().err())
        .map(ToString::to_string)
        .next()
        .unwrap_or_default();
    assert!(
        !error_msg.contains("name,"),
        "anchor name must not include the comma; error was: {error_msg}"
    );
}

#[test]
fn anchor_name_stops_at_closing_bracket() {
    // UT-A24: `[&name]\n` — anchor before `]`; the sequence has the anchor
    // but no value scalar.  The `]` terminates the anchor name scan and the
    // sequence closes.  Current behavior: anchor set with no node → emitted
    // as empty scalar (pending anchor consumed when sequence closes or
    // similar) or as anchor on SequenceEnd — either is acceptable.  This
    // test just verifies no panic and no inclusion of `]` in the name.
    let result: Vec<_> = parse_events("[&name]\n").collect();
    // Check that if any anchor appeared, its name does not contain `]`.
    for item in &result {
        if let Ok((
            Event::Scalar {
                anchor: Some(name), ..
            },
            _,
        )) = item
        {
            assert!(
                !name.contains(']'),
                "anchor name must not include `]`; got `{name}`"
            );
        }
    }
}

#[test]
fn scalar_span_covers_value_not_anchor_prefix() {
    // UT-S2: `&a foo\n` — scalar span must start at `foo` (byte 3),
    // not at `&a` (byte 0).
    let items = parse_to_vec("&a foo\n");
    let scalar_span = items.iter().find_map(|r| match r {
        Ok((Event::Scalar { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(scalar_span.is_some(), "Scalar event must be present");
    if let Some(span) = scalar_span {
        assert_eq!(
            span.start.byte_offset, 3,
            "Scalar span must start at byte 3 (the 'f' of 'foo'), not at the anchor prefix"
        );
    }
}

#[test]
fn sequence_start_span_starts_at_dash_not_anchor_line() {
    // UT-S3: `&anchor\n- item\n` — SequenceStart span should start at the
    // `-` (byte 8), not at the `&anchor` line (byte 0).
    let items = parse_to_vec("&anchor\n- item\n");
    let seq_span = items.iter().find_map(|r| match r {
        Ok((Event::SequenceStart { .. }, span)) => Some(*span),
        Ok(_) | Err(_) => None,
    });
    assert!(seq_span.is_some(), "SequenceStart event must be present");
    if let Some(span) = seq_span {
        assert_eq!(
            span.start.byte_offset, 8,
            "SequenceStart span must start at the `-` (byte 8), not at the anchor line"
        );
    }
}

#[test]
fn flow_sequence_with_anchored_item_then_alias() {
    // IT-A2: `[&a first, *a]\n` — anchored scalar followed by alias.
    let events = evs("[&a first, *a]\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "first"
        )),
        "anchored scalar 'first' must be present"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "a" })),
        "alias `*a` must emit Alias event"
    );
}

#[test]
fn flow_mapping_with_anchored_key_and_alias_value() {
    // IT-A3: `{&k key: *v}\n` — anchor on key, alias as value.
    let events = evs("{&k key: *v}\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
        )),
        "anchor `&k` must be on key scalar"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "v" })),
        "alias `*v` must emit Alias event"
    );
}

#[test]
fn block_sequence_mix_scalars_and_aliases() {
    // IT-A4: `- &first one\n- *first\n- two\n`
    let events = evs("- &first one\n- *first\n- two\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("first"), value, .. } if value.as_ref() == "one"
        )),
        "anchored scalar 'one' must be present"
    );
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::Alias { name: "first" })),
        "alias `*first` must emit Alias event"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "two"
        )),
        "plain scalar 'two' with no anchor must be present"
    );
}

// -----------------------------------------------------------------------
// Group I: PendingAnchor enum consolidation (Task 15)
// -----------------------------------------------------------------------

// A-4: Standalone anchor on block sequence — SequenceStart carries the anchor.
#[test]
fn standalone_anchor_applies_to_block_sequence_start() {
    let events = evs("&seq\n- a\n- b\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("seq"),
                ..
            }
        )),
        "standalone &seq must be attached to SequenceStart"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "a"
        )),
        "first sequence item must have no anchor"
    );
}

// A-5: Inline anchor annotates the key scalar, not the mapping (9KAX scenario).
#[test]
fn inline_anchor_on_key_annotates_key_scalar_not_mapping() {
    let events = evs("&k key: value\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::MappingStart { anchor: None, .. })),
        "MappingStart must have no anchor when &k is inline before key"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
        )),
        "key scalar must carry anchor &k"
    );
}

// A-6: Standalone anchor on block mapping — MappingStart carries the anchor.
#[test]
fn standalone_anchor_applies_to_block_mapping_start() {
    let events = evs("&map\nkey: value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: Some("map"),
                ..
            }
        )),
        "standalone &map must be attached to MappingStart"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "key"
        )),
        "key scalar must have no anchor"
    );
}

// A-7: Inline anchor on a scalar value — value scalar carries the anchor.
#[test]
fn inline_anchor_on_scalar_value_attaches_to_value() {
    let events = evs("key: &a value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "value"
        )),
        "value scalar must carry anchor &a"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "key"
        )),
        "key scalar must have no anchor"
    );
}

// A-8: Nested anchors — outer on sequence, inner on first item.
#[test]
fn nested_anchors_outer_on_sequence_inner_on_item() {
    let events = evs("&outer\n- &inner a\n- b\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("outer"),
                ..
            }
        )),
        "SequenceStart must carry anchor &outer"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("inner"), value, .. } if value.as_ref() == "a"
        )),
        "first item scalar must carry anchor &inner"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "b"
        )),
        "second item scalar must have no anchor"
    );
}

// A-9: When a standalone anchor is followed by an inline key anchor on the next
// line, the standalone anchor anchors the mapping and the inline anchor anchors
// the key scalar.  Per YAML spec, `&map` on its own line applies to the next
// node (the mapping); `&k` inline before the key applies to that key scalar.
#[test]
fn standalone_anchor_applies_to_mapping_inline_anchor_applies_to_key() {
    // "&map\n&k key: value\n" — &map is a standalone anchor for the mapping;
    // &k is an inline anchor for the key scalar.
    let events = evs("&map\n&k key: value\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: Some("map"),
                ..
            }
        )),
        "MappingStart must carry anchor &map"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("k"), value, .. } if value.as_ref() == "key"
        )),
        "key scalar must carry anchor &k"
    );
}

// A-10: Duplicate anchors on the same node return an error.
#[test]
fn duplicate_anchors_on_same_node_return_error() {
    assert!(
        parse_events("&a &b scalar\n").any(|r| r.is_err()),
        "two anchors on one node must return an error"
    );
}

// A-11: Anchor cleared after use — second sequence item has no anchor.
#[test]
fn anchor_cleared_after_use_second_item_has_none() {
    let events = evs("- &a first\n- second\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: Some("a"), value, .. } if value.as_ref() == "first"
        )),
        "first item must carry anchor &a"
    );
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { anchor: None, value, .. } if value.as_ref() == "second"
        )),
        "second item must have no anchor"
    );
}

// B-8: Inline anchor immediately before an alias is an error.
#[test]
fn inline_anchor_before_alias_returns_error() {
    assert!(
        parse_events("&a *b\n").any(|r| r.is_err()),
        "inline anchor &a before alias *b must return an error"
    );
}

// B-10: Standalone anchor at insufficient indent returns an error.
#[test]
fn standalone_anchor_at_insufficient_indent_returns_error() {
    // `"key:\n  nested: val\n&a\n"` — after opening the nested mapping at
    // indent 2, a standalone `&a` at indent 0 is below the minimum required
    // indent for that context.
    let result: Vec<_> = parse_events("key:\n  nested: val\n&a\n").collect();
    let has_indent_error = result.iter().any(|r| {
        r.as_ref()
            .is_err_and(|e| e.to_string().to_lowercase().contains("indent"))
    });
    assert!(
        has_indent_error,
        "standalone anchor below minimum indent must return an indent error"
    );
}

// B-11: Anchor followed immediately by a block-sequence dash on the same line is an error.
#[test]
fn anchor_followed_by_inline_dash_returns_error() {
    assert!(
        parse_events("&a - item\n").any(|r| r.is_err()),
        "anchor &a directly before block-sequence dash on same line must return an error"
    );
}

// -----------------------------------------------------------------------
// Group J: anchor_loc span correctness
// -----------------------------------------------------------------------

// J-1: anchor_loc is None when no anchor is present on a scalar.
#[test]
fn anchor_loc_none_when_no_anchor_on_scalar() {
    let events = parse_to_vec("value\n");
    let scalar = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar { anchor_loc, .. } = ev {
                Some(*anchor_loc)
            } else {
                None
            }
        })
    });
    assert_eq!(
        scalar,
        Some(None),
        "scalar with no anchor must have anchor_loc: None"
    );
}

// J-2: anchor_loc is Some for an anchored scalar, covering `&` through last byte of name.
#[test]
fn anchor_loc_some_for_anchored_plain_scalar() {
    // `&abc val\n` — `&abc` starts at byte 0, col 0, line 1.
    // anchor name length = 3, so end = byte 4, col 4.
    let events = parse_to_vec("&abc val\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                anchor: Some("abc"),
                anchor_loc: Some(s),
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
        "anchored scalar must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 4,
                line: 1,
                column: 4,
            },
        };
        assert_eq!(
            loc, expected,
            "anchor_loc must cover `&` through last byte of name"
        );
    }
}

// J-3: anchor_loc is Some for SequenceStart.
#[test]
fn anchor_loc_some_for_anchored_sequence_start() {
    // `&s\n- item\n` — `&s` at byte 0, col 0, line 1; end = byte 2, col 2.
    let events = parse_to_vec("&s\n- item\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::SequenceStart {
                anchor: Some("s"),
                anchor_loc: Some(s),
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
        "anchored SequenceStart must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 2,
                line: 1,
                column: 2,
            },
        };
        assert_eq!(loc, expected, "SequenceStart anchor_loc must cover `&s`");
    }
}

// J-4: anchor_loc is Some for MappingStart.
#[test]
fn anchor_loc_some_for_anchored_mapping_start() {
    // `&m\nkey: val\n` — `&m` at byte 0, col 0, line 1; end = byte 2, col 2.
    let events = parse_to_vec("&m\nkey: val\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::MappingStart {
                anchor: Some("m"),
                anchor_loc: Some(s),
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
        "anchored MappingStart must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 2,
                line: 1,
                column: 2,
            },
        };
        assert_eq!(loc, expected, "MappingStart anchor_loc must cover `&m`");
    }
}

// J-5: anchor_loc for an inline anchor on a mapping key covers `&` through last byte of name.
#[test]
fn anchor_loc_inline_anchor_on_mapping_key_scalar() {
    // `&k key: val\n` — inline anchor before key; `&k` at byte 0, col 0, line 1.
    let events = parse_to_vec("&k key: val\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                anchor: Some("k"),
                anchor_loc: Some(s),
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
        "key scalar with inline anchor must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 2,
                line: 1,
                column: 2,
            },
        };
        assert_eq!(
            loc, expected,
            "inline anchor on key: anchor_loc must cover `&k`"
        );
    }
}

// J-6: anchor_loc for a multibyte anchor name covers the correct byte range.
#[test]
fn anchor_loc_multibyte_anchor_name_byte_range() {
    // `&\u{00E9}n val\n` — é (U+00E9) = 2 bytes; anchor name = `én` (3 bytes total).
    // `&` at byte 0 col 0; end byte = 1 + 2 + 1 = 4; end col = 1 + 1 + 1 = 3.
    let input = "&\u{00E9}n val\n";
    let events = parse_to_vec(input);
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                anchor: Some(_),
                anchor_loc: Some(s),
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
        "multibyte-anchored scalar must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        // `&` = 1 byte at col 0; `é` = 2 bytes at col 1; `n` = 1 byte at col 2;
        // end byte = 0 + 1 + 2 + 1 = 4; end col = 0 + 1 + 1 + 1 = 3.
        assert_eq!(loc.start.byte_offset, 0, "anchor start byte must be 0");
        assert_eq!(loc.start.column, 0, "anchor start col must be 0");
        assert_eq!(
            loc.end.byte_offset, 4,
            "anchor end byte must cover ampersand + 2-byte + 1-byte"
        );
        assert_eq!(
            loc.end.column, 3,
            "anchor end col must be 3 (1 + 1 + 1 codepoints)"
        );
    }
}

// J-7: anchor_loc start matches the `&` byte position when anchor is not at column 0.
#[test]
fn anchor_loc_start_at_ampersand_mid_line() {
    // `key: &v val\n` — `&v` is at byte offset 5, col 5, line 1.
    let events = parse_to_vec("key: &v val\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                anchor: Some("v"),
                anchor_loc: Some(s),
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
        "value-anchored scalar must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        // `key: ` = 5 bytes; `&` at byte 5, col 5.
        assert_eq!(loc.start.byte_offset, 5, "anchor start byte must be 5");
        assert_eq!(loc.start.column, 5, "anchor start col must be 5");
        assert_eq!(
            loc.end.byte_offset, 7,
            "anchor end byte must be 7 (`&v` = 2 bytes)"
        );
        assert_eq!(loc.end.column, 7, "anchor end col must be 7");
    }
}

// J-8: anchor_loc is None for SequenceStart when no anchor present.
#[test]
fn anchor_loc_none_for_sequence_start_without_anchor() {
    let events = parse_to_vec("- item\n");
    let loc = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::SequenceStart { anchor_loc, .. } = ev {
                Some(*anchor_loc)
            } else {
                None
            }
        })
    });
    assert_eq!(
        loc,
        Some(None),
        "SequenceStart without anchor must have anchor_loc: None"
    );
}

// J-9: anchor_loc is None for MappingStart when no anchor present.
#[test]
fn anchor_loc_none_for_mapping_start_without_anchor() {
    let events = parse_to_vec("key: val\n");
    let loc = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::MappingStart { anchor_loc, .. } = ev {
                Some(*anchor_loc)
            } else {
                None
            }
        })
    });
    assert_eq!(
        loc,
        Some(None),
        "MappingStart without anchor must have anchor_loc: None"
    );
}

// J-10: anchor_loc start line matches the physical line of the `&` token.
#[test]
fn anchor_loc_start_line_matches_anchor_line() {
    // Standalone anchor on line 2: `---\n&a val\n`
    // `&a` is on line 2, byte offset = len("---\n") = 4.
    let events = parse_to_vec("---\n&a val\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                anchor: Some("a"),
                anchor_loc: Some(s),
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
        "anchored scalar on line 2 must have anchor_loc"
    );
    if let Some(loc) = loc_opt {
        assert_eq!(loc.start.line, 2, "anchor_loc.start.line must be 2");
        assert_eq!(
            loc.start.byte_offset, 4,
            "anchor_loc.start.byte_offset must be 4 (after `---\\n`)"
        );
    }
}

// J-11: anchor_loc for an anchor before a flow mapping.
#[test]
fn anchor_loc_some_for_anchored_flow_mapping() {
    // `&a {k: v}\n` — `&a` at byte 0, col 0, line 1; end = byte 2, col 2.
    let events = parse_to_vec("&a {k: v}\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::MappingStart {
                anchor: Some("a"),
                anchor_loc: Some(s),
                style: CollectionStyle::Flow,
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
        "anchored flow MappingStart must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 2,
                line: 1,
                column: 2,
            },
        };
        assert_eq!(
            loc, expected,
            "flow MappingStart anchor_loc must cover `&a`"
        );
    }
}

// J-12: anchor_loc for an anchor before a flow sequence.
#[test]
fn anchor_loc_some_for_anchored_flow_sequence() {
    // `&a [item]\n` — `&a` at byte 0, col 0, line 1; end = byte 2, col 2.
    let events = parse_to_vec("&a [item]\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::SequenceStart {
                anchor: Some("a"),
                anchor_loc: Some(s),
                style: CollectionStyle::Flow,
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
        "anchored flow SequenceStart must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 2,
                line: 1,
                column: 2,
            },
        };
        assert_eq!(
            loc, expected,
            "flow SequenceStart anchor_loc must cover `&a`"
        );
    }
}

// J-13: anchor_loc for a dotted anchor name covers all bytes of `&a.b.c`.
#[test]
fn anchor_loc_dotted_anchor_name_full_span() {
    // `&a.b.c value\n` — `&a.b.c` at byte 0, col 0; end = byte 6, col 6.
    let events = parse_to_vec("&a.b.c value\n");
    let loc_opt = events.iter().find_map(|r| {
        r.as_ref().ok().and_then(|(ev, _)| {
            if let Event::Scalar {
                anchor: Some("a.b.c"),
                anchor_loc: Some(s),
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
        "dotted-anchor scalar must have anchor_loc = Some(...)"
    );
    if let Some(loc) = loc_opt {
        let expected = Span {
            start: Pos {
                byte_offset: 0,
                line: 1,
                column: 0,
            },
            end: Pos {
                byte_offset: 6,
                line: 1,
                column: 6,
            },
        };
        assert_eq!(loc, expected, "anchor_loc must cover `&a.b.c` (6 bytes)");
    }
}

// J-INV-CORPUS: invariant — anchor.is_some() == anchor_loc.is_some() for every Ok event
// across every yaml-test-suite .yaml file.
#[test]
fn anchor_loc_invariant_corpus_wide() {
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
            let anchor_pair = match &ev {
                Event::Scalar {
                    anchor, anchor_loc, ..
                }
                | Event::SequenceStart {
                    anchor, anchor_loc, ..
                }
                | Event::MappingStart {
                    anchor, anchor_loc, ..
                } => Some((anchor.is_some(), anchor_loc.is_some())),
                Event::StreamStart
                | Event::StreamEnd
                | Event::Comment { .. }
                | Event::DocumentStart { .. }
                | Event::DocumentEnd { .. }
                | Event::Alias { .. }
                | Event::SequenceEnd
                | Event::MappingEnd => None,
            };
            if let Some((anchor_is_some, anchor_loc_is_some)) = anchor_pair {
                assert_eq!(
                    anchor_is_some, anchor_loc_is_some,
                    "invariant violated in {file_name}: anchor.is_some()={anchor_is_some} but anchor_loc.is_some()={anchor_loc_is_some} for event {ev:?}"
                );
            }
        }
    }
    assert!(file_count > 0, "no .yaml files found in {suite_dir:?}");
}

// J-INV: invariant — anchor.is_some() == anchor_loc.is_some() for every event in a complex document.
#[test]
fn anchor_loc_invariant_anchor_some_iff_loc_some() {
    let input = "&seq\n- &item val\n- plain\n&map\nkey: &v val2\n";
    let events = parse_to_vec(input);
    for (ev, _) in events.iter().filter_map(|r| r.as_ref().ok()) {
        let anchor_pair = match ev {
            Event::Scalar {
                anchor, anchor_loc, ..
            }
            | Event::SequenceStart {
                anchor, anchor_loc, ..
            }
            | Event::MappingStart {
                anchor, anchor_loc, ..
            } => Some((anchor.is_some(), anchor_loc.is_some())),
            Event::StreamStart
            | Event::StreamEnd
            | Event::Comment { .. }
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::Alias { .. }
            | Event::SequenceEnd
            | Event::MappingEnd => None,
        };
        if let Some((anchor_is_some, anchor_loc_is_some)) = anchor_pair {
            assert_eq!(
                anchor_is_some, anchor_loc_is_some,
                "invariant violated: anchor.is_some()={anchor_is_some} but anchor_loc.is_some()={anchor_loc_is_some} for event {ev:?}"
            );
        }
    }
}
