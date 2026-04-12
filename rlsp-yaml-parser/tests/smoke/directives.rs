use rstest::rstest;

use super::*;

// -----------------------------------------------------------------------
// Group A — %YAML directive
// -----------------------------------------------------------------------

// A-1 through A-3: %YAML directive version propagated to DocumentStart.version.
#[rstest]
// A-1: %YAML 1.2 produces version Some((1, 2)).
#[case::yaml_1_2_propagated("%YAML 1.2\n---\nscalar\n", Some((1, 2)))]
// A-2: %YAML 1.1 produces version Some((1, 1)).
#[case::yaml_1_1_propagated("%YAML 1.1\n---\nscalar\n", Some((1, 1)))]
// A-3: No %YAML directive produces version None.
#[case::no_yaml_directive_version_is_none("---\nscalar\n", None)]
// A-3b: Non-standard version %YAML 1.3 is accepted without validation.
#[case::yaml_non_standard_version_accepted("%YAML 1.3\n---\nscalar\n", Some((1, 3)))]
fn yaml_directive_version_propagated_to_document_start(
    #[case] input: &str,
    #[case] expected_version: Option<(u8, u8)>,
) {
    let events = event_variants(input);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart { version, .. } if *version == expected_version
        )),
        "%YAML directive must produce DocumentStart with version {expected_version:?} for input: {input:?}"
    );
}

// A-4, A-5, A-6, A-8: Malformed or disallowed %YAML directives return at least one error.
#[rstest]
// A-4: Missing version number after %YAML.
#[case::missing_version_returns_error("%YAML\n---\nscalar\n")]
// A-5: Non-numeric version (e.g., %YAML abc) is rejected.
#[case::non_numeric_version_returns_error("%YAML abc\n---\nscalar\n")]
// A-6: Unsupported major version 2 is rejected (only major 1 is supported).
#[case::major_version_2_returns_error("%YAML 2.0\n---\nscalar\n")]
// A-8: Duplicate %YAML directives in the same preamble are rejected.
#[case::duplicate_yaml_directive_returns_error("%YAML 1.2\n%YAML 1.2\n---\nscalar\n")]
fn yaml_directive_invalid_input_returns_error(#[case] input: &str) {
    assert!(
        has_error(input),
        "invalid %YAML directive must return an error for input: {input:?}"
    );
}

// A-9: %YAML directive scope resets between documents.
#[test]
fn yaml_directive_scope_resets_between_documents() {
    let events = event_variants("%YAML 1.2\n---\nfirst\n...\n---\nsecond\n");
    let versions: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::DocumentStart { version, .. } = e {
                Some(*version)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        versions,
        [Some((1, 2)), None],
        "first doc must have version Some((1,2)); second doc (no directive) must have version None"
    );
}

// -----------------------------------------------------------------------
// Group B — %TAG directive
// -----------------------------------------------------------------------

// B-1: %TAG directive populates tag_directives field.
#[test]
fn tag_directive_propagated_to_document_start() {
    let events = event_variants("%TAG !foo! tag:example.com,2026:\n---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart { tag_directives, .. }
                if tag_directives.iter().any(|(h, p)| h == "!foo!" && p == "tag:example.com,2026:")
        )),
        "%TAG must populate DocumentStart.tag_directives with the declared handle and prefix"
    );
}

// B-2: No %TAG directives → tag_directives is empty.
#[test]
fn no_tag_directive_produces_empty_tag_directives() {
    let events = event_variants("---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart { tag_directives, .. } if tag_directives.is_empty()
        )),
        "absent %TAG directives must produce DocumentStart with empty tag_directives"
    );
}

// B-3: Multiple %TAG directives accumulate.
#[test]
fn multiple_tag_directives_all_present_in_document_start() {
    let events = event_variants("%TAG !a! prefix-a:\n%TAG !b! prefix-b:\n---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart { tag_directives, .. }
                if tag_directives.iter().any(|(h, _)| h == "!a!")
                    && tag_directives.iter().any(|(h, _)| h == "!b!")
        )),
        "multiple %TAG directives must all appear in DocumentStart.tag_directives"
    );
}

// B-4: Duplicate %TAG handle returns error.
#[test]
fn duplicate_tag_handle_returns_error() {
    assert!(
        has_error("%TAG !foo! prefix-a:\n%TAG !foo! prefix-b:\n---\nscalar\n"),
        "duplicate %TAG handle must return an error"
    );
}

// B-5: %TAG directive scope resets between documents.
#[test]
fn tag_directive_scope_resets_between_documents() {
    let events = event_variants("%TAG !foo! prefix-a:\n---\nfirst\n...\n---\nsecond\n");
    let directives_per_doc: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::DocumentStart { tag_directives, .. } = e {
                Some(tag_directives.clone())
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        directives_per_doc.len(),
        2,
        "expected two DocumentStart events"
    );
    assert!(
        directives_per_doc.first().is_some_and(|d| !d.is_empty()),
        "first doc must include the !foo! tag directive"
    );
    assert!(
        directives_per_doc.get(1).is_some_and(Vec::is_empty),
        "second doc (no directives) must have empty tag_directives"
    );
}

// -----------------------------------------------------------------------
// Group C — Default handle expansion (no %TAG override)
// -----------------------------------------------------------------------

// C-1: `!!str` expands to `tag:yaml.org,2002:str` without any %TAG.
#[test]
fn default_handle_expands_to_core_schema_prefix() {
    let events = evs("!!str hello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:str"
        )),
        "!!str must expand to 'tag:yaml.org,2002:str' using the default !! handle"
    );
}

// C-2: `!! val` (empty suffix) expands to `tag:yaml.org,2002:`.
#[test]
fn default_handle_empty_suffix_expands_to_prefix_only() {
    let events = evs("!! val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. } if t.as_ref() == "tag:yaml.org,2002:"
        )),
        "!! with empty suffix must expand to 'tag:yaml.org,2002:'"
    );
}

// -----------------------------------------------------------------------
// Group D — Custom %TAG handle resolution
// -----------------------------------------------------------------------

// D-1: Custom handle resolves scalar tag at scan time.
#[test]
fn custom_tag_handle_resolves_scalar_tag() {
    let events = evs("%TAG !e! tag:example.com,2026:\n---\n!e!foo bar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. }
                if t.as_ref() == "tag:example.com,2026:foo"
        )),
        "!e!foo with %TAG !e! tag:example.com,2026: must resolve to 'tag:example.com,2026:foo'"
    );
}

// D-2: %TAG overrides the default !! handle.
#[test]
fn percent_tag_overrides_default_double_bang_handle() {
    let events = evs("%TAG !! tag:custom.org,2026:\n---\n!!str hello\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. }
                if t.as_ref() == "tag:custom.org,2026:str"
        )),
        "%TAG !! override must cause !!str to resolve to 'tag:custom.org,2026:str'"
    );
}

// D-3: Undeclared named handle returns error.
#[test]
fn undeclared_named_handle_returns_error() {
    assert!(
        has_error("!e!foo bar\n"),
        "using !e! handle without %TAG declaration must return an error"
    );
}

// D-4: Custom handle resolves on sequence tag.
#[test]
fn custom_tag_handle_resolves_sequence_tag() {
    let events = evs("%TAG !e! tag:example.com,2026:\n---\n!e!seq\n- item\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart { tag: Some(t), .. }
                if t.as_ref() == "tag:example.com,2026:seq"
        )),
        "!e!seq on block sequence must resolve to 'tag:example.com,2026:seq'"
    );
}

// D-5: Custom handle resolves on mapping tag.
#[test]
fn custom_tag_handle_resolves_mapping_tag() {
    let events = evs("%TAG !e! tag:example.com,2026:\n---\n!e!map\nkey: val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart { tag: Some(t), .. }
                if t.as_ref() == "tag:example.com,2026:map"
        )),
        "!e!map on block mapping must resolve to 'tag:example.com,2026:map'"
    );
}

// -----------------------------------------------------------------------
// Group E — Verbatim tags (unchanged by resolve_tag)
// -----------------------------------------------------------------------

// E-1: Verbatim tag `!<URI>` is stored as bare URI (no angle brackets).
#[test]
fn verbatim_tag_stored_as_bare_uri() {
    let events = evs("!<tag:example.com,2026:str> val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. }
                if t.as_ref() == "tag:example.com,2026:str"
        )),
        "verbatim tag must be stored as bare URI without angle brackets"
    );
}

// E-2: Local tag `!suffix` is stored as-is (no expansion).
#[test]
fn local_tag_stored_as_is() {
    let events = evs("!foo val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. } if t.as_ref() == "!foo"
        )),
        "local tag !foo must be stored as '!foo' without expansion"
    );
}

// -----------------------------------------------------------------------
// Group F — Directive scope per document
// -----------------------------------------------------------------------

// F-1: Directive scope is independent per document in a multi-doc stream.
#[test]
fn directive_scope_is_independent_per_document() {
    // Doc 1: %TAG !e! prefix-a:, uses !e!type
    // Doc 2: %TAG !e! prefix-b:, uses !e!type
    // Both should resolve to different prefixes.
    let input =
        "%TAG !e! prefix-a:\n---\n!e!type val1\n...\n%TAG !e! prefix-b:\n---\n!e!type val2\n";
    let events = evs(input);
    let tags: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::Scalar {
                tag: Some(t),
                value,
                ..
            } = e
            {
                if value.as_ref() == "val1" || value.as_ref() == "val2" {
                    return Some((value.as_ref().to_owned(), t.as_ref().to_owned()));
                }
            }
            None
        })
        .collect();
    assert_eq!(tags.len(), 2, "expected two scalars with tags");
    assert!(
        tags.iter()
            .any(|(v, t)| v == "val1" && t == "prefix-a:type"),
        "doc 1 !e!type must resolve to 'prefix-a:type'"
    );
    assert!(
        tags.iter()
            .any(|(v, t)| v == "val2" && t == "prefix-b:type"),
        "doc 2 !e!type must resolve to 'prefix-b:type'"
    );
}

// F-2: Directive from doc 1 is not visible in doc 2.
#[test]
fn directive_from_first_doc_not_visible_in_second() {
    // Doc 1: %TAG !e! prefix:, doc 2: no directive — !e!type must error.
    let input = "%TAG !e! prefix:\n---\nscalar\n...\n---\n!e!type val\n";
    assert!(
        has_error(input),
        "handle declared in doc 1 must not be visible in doc 2"
    );
}

// -----------------------------------------------------------------------
// Group G — Multi-document streams
// -----------------------------------------------------------------------

// G-1: Multi-doc stream without directives produces multiple DocumentStart events.
#[test]
fn multi_doc_stream_without_directives() {
    let events = event_variants("---\nfirst\n...\n---\nsecond\n");
    let doc_starts = events
        .iter()
        .filter(|e| matches!(e, Event::DocumentStart { .. }))
        .count();
    assert_eq!(
        doc_starts, 2,
        "two documents must produce two DocumentStart events"
    );
}

// G-2: Each document in a multi-doc stream gets its own version field.
#[test]
fn multi_doc_stream_each_doc_gets_its_own_version() {
    let events = event_variants("%YAML 1.2\n---\nfirst\n...\n%YAML 1.3\n---\nsecond\n");
    let versions: Vec<_> = events
        .iter()
        .filter_map(|e| {
            if let Event::DocumentStart { version, .. } = e {
                Some(*version)
            } else {
                None
            }
        })
        .collect();
    assert_eq!(
        versions,
        [Some((1, 2)), Some((1, 3))],
        "each document must carry its own %YAML version"
    );
}

// -----------------------------------------------------------------------
// Group H — DocumentStart completeness
// -----------------------------------------------------------------------

// H-1: Explicit document (with `---`) sets explicit: true.
#[test]
fn explicit_document_marker_sets_explicit_true() {
    let events = event_variants("---\nscalar\n");
    assert!(
        events
            .iter()
            .any(|e| matches!(e, Event::DocumentStart { explicit: true, .. })),
        "--- marker must produce DocumentStart with explicit: true"
    );
}

// H-2: Bare document (without `---`) sets explicit: false.
#[test]
fn bare_document_sets_explicit_false() {
    let events = event_variants("scalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart {
                explicit: false,
                ..
            }
        )),
        "bare document must produce DocumentStart with explicit: false"
    );
}

// H-3: %YAML + %TAG together populate both fields.
#[test]
fn yaml_and_tag_directives_both_present_in_document_start() {
    let events = event_variants("%YAML 1.2\n%TAG !e! prefix:\n---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart {
                version: Some((1, 2)),
                tag_directives,
                ..
            } if !tag_directives.is_empty()
        )),
        "%YAML and %TAG must both be present in DocumentStart"
    );
}

// -----------------------------------------------------------------------
// Group I — Unknown directives
// -----------------------------------------------------------------------

// I-1: Unknown directive is silently skipped (does not return an error).
#[test]
fn unknown_directive_is_silently_skipped() {
    assert!(
        !has_error("%FOO bar baz\n---\nscalar\n"),
        "unknown directive must be silently skipped, not return an error"
    );
}

// I-2: Unknown directive does not pollute DocumentStart fields.
#[test]
fn unknown_directive_does_not_affect_document_start() {
    let events = event_variants("%FOO bar\n---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart { version: None, tag_directives, .. }
                if tag_directives.is_empty()
        )),
        "unknown directive must not affect DocumentStart fields"
    );
}

// -----------------------------------------------------------------------
// Group J — Span correctness
// -----------------------------------------------------------------------

// J-1: DocumentStart span covers the `---` marker when explicit.
#[test]
fn explicit_document_start_span_covers_dashes() {
    let items = parse_to_vec("---\nscalar\n");
    let doc_start_span = items.iter().find_map(|r| match r {
        Ok((Event::DocumentStart { explicit: true, .. }, span)) => Some(*span),
        _ => None,
    });
    assert!(
        doc_start_span.is_some(),
        "expected an explicit DocumentStart"
    );
    if let Some(span) = doc_start_span {
        assert_eq!(
            span.start.byte_offset, 0,
            "DocumentStart span must start at byte 0"
        );
        assert_eq!(
            span.end.byte_offset, 3,
            "DocumentStart span must end after '---' (byte 3)"
        );
    }
}

// -----------------------------------------------------------------------
// Group K — Edge cases
// -----------------------------------------------------------------------

// K-1: %TAG directive with no prefix field (line ends after handle) returns error.
#[test]
fn tag_directive_missing_prefix_returns_error() {
    assert!(
        has_error("%TAG !foo!\n---\nscalar\n"),
        "%TAG with no prefix field must return an error"
    );
}

// K-2: %TAG directive whose prefix is the two-char literal `""` is
// accepted (non-empty prefix consisting of two ASCII quote characters).
#[test]
fn tag_directive_with_double_quote_prefix_is_accepted() {
    assert!(
        !has_error("%TAG !e! \"\"\n---\nscalar\n"),
        "%TAG with double-quote prefix must be accepted"
    );
}

// K-3: Directive count at limit is accepted.
#[test]
fn directive_count_at_limit_is_accepted() {
    // Build exactly MAX_DIRECTIVES_PER_DOC directives with distinct handles.
    use std::fmt::Write as _;
    let mut input = String::new();
    for i in 0..MAX_DIRECTIVES_PER_DOC {
        let _ = writeln!(input, "%TAG !h{i}! prefix{i}:");
    }
    input.push_str("---\nscalar\n");
    assert!(
        !has_error(&input),
        "exactly MAX_DIRECTIVES_PER_DOC directives must be accepted"
    );
}

// K-4: Directive count exceeding limit returns error.
#[test]
fn directive_count_exceeding_limit_returns_error() {
    use std::fmt::Write as _;
    let mut input = String::new();
    for i in 0..=MAX_DIRECTIVES_PER_DOC {
        let _ = writeln!(input, "%TAG !h{i}! prefix{i}:");
    }
    input.push_str("---\nscalar\n");
    assert!(
        has_error(&input),
        "more than MAX_DIRECTIVES_PER_DOC directives must return an error"
    );
}

// K-5: Tag handle at byte limit is accepted.
#[test]
fn tag_handle_at_byte_limit_is_accepted() {
    // Handle is `!` + (MAX_TAG_HANDLE_BYTES - 3) inner chars + `!`
    // Total handle bytes = MAX_TAG_HANDLE_BYTES.
    let inner = "a".repeat(MAX_TAG_HANDLE_BYTES.saturating_sub(3));
    let handle = format!("!{inner}!");
    let input = format!("%TAG {handle} prefix:\n---\n!{inner}!suffix val\n");
    assert!(
        !has_error(&input),
        "tag handle at MAX_TAG_HANDLE_BYTES must be accepted"
    );
}

// K-6: Tag handle exceeding byte limit returns error.
#[test]
fn tag_handle_exceeding_byte_limit_returns_error() {
    let inner = "a".repeat(MAX_TAG_HANDLE_BYTES);
    let handle = format!("!{inner}!");
    let input = format!("%TAG {handle} prefix:\n---\nscalar\n");
    assert!(
        has_error(&input),
        "tag handle exceeding MAX_TAG_HANDLE_BYTES must return an error"
    );
}

// K-7: Tag prefix at exactly MAX_TAG_LEN bytes is accepted.
#[test]
fn tag_prefix_at_byte_limit_is_accepted() {
    let prefix = "a".repeat(MAX_TAG_LEN);
    let input = format!("%TAG !e! {prefix}\n---\nscalar\n");
    assert!(
        !has_error(&input),
        "tag prefix at MAX_TAG_LEN must be accepted"
    );
}

// K-8: Tag prefix exceeding MAX_TAG_LEN by one byte returns error.
#[test]
fn tag_prefix_exceeding_byte_limit_returns_error() {
    let prefix = "a".repeat(MAX_TAG_LEN + 1);
    let input = format!("%TAG !e! {prefix}\n---\nscalar\n");
    assert!(
        has_error(&input),
        "tag prefix exceeding MAX_TAG_LEN must return an error"
    );
}

// K-9: Control character in %TAG prefix returns error.
#[test]
fn tag_prefix_with_control_character_returns_error() {
    assert!(
        has_error("%TAG !e! tag:\x01example.com\n---\nscalar\n"),
        "control character in %TAG prefix must return an error"
    );
}

// -----------------------------------------------------------------------
// Group L — directive+comment interaction and directive-without-marker
// -----------------------------------------------------------------------

// L-1: Comment after %YAML directive does not clobber version.
#[test]
fn yaml_directive_survives_trailing_comment() {
    let events = event_variants("%YAML 1.2\n# comment\n---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart {
                version: Some((1, 2)),
                ..
            }
        )),
        "%YAML 1.2 version must survive a following comment line"
    );
}

// L-2: Comment before %YAML directive does not clobber version.
#[test]
fn yaml_directive_survives_leading_comment() {
    let events = event_variants("# comment\n%YAML 1.2\n---\nscalar\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::DocumentStart {
                version: Some((1, 2)),
                ..
            }
        )),
        "%YAML 1.2 version must survive a preceding comment line"
    );
}

// L-3: Comments interspersed between %YAML and %TAG directives preserve both.
#[test]
fn directives_survive_interspersed_comments() {
    let events = evs("%YAML 1.2\n# a\n# b\n%TAG !e! prefix:\n---\n!e!foo val\n");
    // DocumentStart must carry version and tag_directives.
    let doc_start = events.iter().find(|e| {
        matches!(
            e,
            Event::DocumentStart {
                version: Some((1, 2)),
                ..
            }
        )
    });
    assert!(
        doc_start.is_some(),
        "DocumentStart must have version Some((1,2)) when comments interspersed"
    );
    if let Some(Event::DocumentStart { tag_directives, .. }) = doc_start {
        assert!(
            tag_directives
                .iter()
                .any(|(h, p)| h == "!e!" && p == "prefix:"),
            "DocumentStart must carry !e! tag directive when comments interspersed"
        );
    }
    // Scalar tag must be resolved.
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. } if t.as_ref() == "prefix:foo"
        )),
        "!e!foo must resolve to prefix:foo when comments interspersed"
    );
}

// L-4: Comment between %TAG and `---` does not break tag resolution.
#[test]
fn tag_directive_survives_trailing_comment() {
    let events = evs("%TAG !e! tag:example.com:\n# banner\n---\n!e!foo val\n");
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Scalar { tag: Some(t), .. } if t.as_ref() == "tag:example.com:foo"
        )),
        "!e!foo must resolve to tag:example.com:foo when comment follows %TAG"
    );
}

// L-5: %YAML directive without `---` returns error.
#[test]
fn yaml_directive_without_marker_returns_error() {
    assert!(
        has_error("%YAML 1.2\nscalar\n"),
        "%YAML directive without --- must return an error"
    );
}

// L-6: %TAG directive without `---` returns error.
#[test]
fn tag_directive_without_marker_returns_error() {
    assert!(
        has_error("%TAG !e! prefix:\nscalar\n"),
        "%TAG directive without --- must return an error"
    );
}

// L-7: Reserved directive without `---` returns error.
#[test]
fn reserved_directive_without_marker_returns_error() {
    assert!(
        has_error("%FOO bar\nscalar\n"),
        "reserved directive without --- must return an error"
    );
}

// L-8: %YAML directive followed by orphan `...` (not `---`) returns error.
#[test]
fn yaml_directive_followed_by_document_end_returns_error() {
    assert!(
        has_error("%YAML 1.2\n...\nscalar\n"),
        "%YAML directive followed by ... (not ---) must return an error"
    );
}

// L-9: Multi-doc with comments between directives and marker preserves scope
// isolation across documents.
#[test]
fn multi_doc_directive_scope_isolated_through_comments() {
    let events = evs("%YAML 1.2\n# one\n---\nfirst\n...\n%YAML 1.1\n# two\n---\nsecond\n");
    let doc_starts: Vec<_> = events
        .iter()
        .filter(|e| matches!(e, Event::DocumentStart { .. }))
        .collect();
    assert_eq!(doc_starts.len(), 2, "expected two DocumentStart events");
    assert!(
        matches!(
            doc_starts.first(),
            Some(Event::DocumentStart {
                version: Some((1, 2)),
                ..
            })
        ),
        "first doc must have version Some((1, 2))"
    );
    assert!(
        matches!(
            doc_starts.get(1),
            Some(Event::DocumentStart {
                version: Some((1, 1)),
                ..
            })
        ),
        "second doc must have version Some((1, 1))"
    );
}

// -----------------------------------------------------------------------
// Group M — %YAML trailing garbage and %TAG handle shape validation
// -----------------------------------------------------------------------

// M-1: %YAML directive with trailing garbage returns error.
#[test]
fn yaml_directive_trailing_garbage_returns_error() {
    assert!(
        has_error("%YAML 1.2 garbage\n---\nscalar\n"),
        "%YAML with trailing non-comment garbage must return an error"
    );
}

// M-2: %YAML directive with trailing comment is accepted.
#[test]
fn yaml_directive_trailing_comment_is_accepted() {
    assert!(
        !has_error("%YAML 1.2 # a comment\n---\nscalar\n"),
        "%YAML with trailing comment must be accepted"
    );
}

// M-3: %TAG handle not starting with `!` returns error.
#[test]
fn tag_handle_without_leading_bang_returns_error() {
    assert!(
        has_error("%TAG noBang prefix:\n---\nscalar\n"),
        "%TAG handle not starting with ! must return an error"
    );
}

// M-4: %TAG named handle missing trailing `!` returns error.
#[test]
fn tag_handle_missing_trailing_bang_returns_error() {
    assert!(
        has_error("%TAG !a prefix:\n---\nscalar\n"),
        "%TAG named handle missing trailing ! must return an error"
    );
}

// M-5: %TAG handle with three bangs (`!!!`) returns error.
#[test]
fn tag_handle_three_bangs_returns_error() {
    assert!(
        has_error("%TAG !!! prefix:\n---\nscalar\n"),
        "%TAG handle !!! must return an error"
    );
}

// M-6: %TAG primary handle `!` is accepted.
#[test]
fn tag_handle_primary_is_accepted() {
    assert!(
        !has_error("%TAG ! prefix:\n---\nscalar\n"),
        "%TAG primary handle ! must be accepted"
    );
}

// M-7: %TAG secondary handle `!!` is accepted.
#[test]
fn tag_handle_secondary_is_accepted() {
    assert!(
        !has_error("%TAG !! prefix:\n---\nscalar\n"),
        "%TAG secondary handle !! must be accepted"
    );
}

// M-8: %TAG named handle `!foo!` is accepted.
#[test]
fn tag_handle_named_is_accepted() {
    assert!(
        !has_error("%TAG !foo! prefix:\n---\nscalar\n"),
        "%TAG named handle !foo! must be accepted"
    );
}
