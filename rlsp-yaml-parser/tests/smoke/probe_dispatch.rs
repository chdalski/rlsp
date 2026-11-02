use super::*;

// -----------------------------------------------------------------------
// Group Q: Pending-state handoff — tag+anchor both standalone, then
// sequence/mapping entry.  After the reorder, the sequence/mapping probes
// fire before the anchor/tag probes, so the pending-state must survive
// across two Continue cycles before the structural probe fires.
// -----------------------------------------------------------------------

// Q-5b: tag first, then anchor, then sequence entry — both properties
// attach to SequenceStart (reverse ordering of the T-11 test in mod tags).
#[test]
fn tag_first_anchor_second_standalone_both_propagate_to_sequence_start() {
    let input = "!!seq\n&myseq\n- a\n";
    let events = evs(input);
    assert!(
        events.iter().any(|e| {
            matches!(e, Event::SequenceStart { .. })
                && e.anchor() == Some("myseq")
                && e.tag() == Some("tag:yaml.org,2002:seq")
        }),
        "SequenceStart must carry both anchor myseq and tag:yaml.org,2002:seq (tag-first order)"
    );
}

// Q-6: anchor first, then tag, then mapping entry — both properties attach
// to MappingStart.
#[test]
fn anchor_first_tag_second_standalone_both_propagate_to_mapping_start() {
    let input = "&mymap\n!!map\nkey: val\n";
    let events = evs(input);
    assert!(
        events.iter().any(|e| {
            matches!(e, Event::MappingStart { .. })
                && e.anchor() == Some("mymap")
                && e.tag() == Some("tag:yaml.org,2002:map")
        }),
        "MappingStart must carry both anchor mymap and tag:yaml.org,2002:map"
    );
}

// -----------------------------------------------------------------------
// Group R: Inline anchor + block-sequence dash — error path variants.
// R-1 (`&a - item\n`) is already covered in mod anchors_and_aliases.
// R-2 and R-3 are new.
// -----------------------------------------------------------------------

// R-2: tab after dash in inline anchor is also rejected.
#[test]
fn anchor_followed_by_inline_dash_tab_returns_error() {
    assert!(
        has_error("&a -\titem\n"),
        "anchor &a with inline `- <tab>item` must return an error"
    );
}

// R-3: bare dash (no trailing content) after inline anchor is rejected.
#[test]
fn anchor_followed_by_inline_bare_dash_returns_error() {
    assert!(
        has_error("&a -\n"),
        "anchor &a with inline bare `-` must return an error"
    );
}

// -----------------------------------------------------------------------
// Group R: no line of the block dispatcher is consumed without a trace
// -----------------------------------------------------------------------
// Every line below a context either yields an event that carries its text or
// ends the stream with an error.  A line the dispatcher consumes silently is
// text the formatter deletes.

#[test]
fn no_block_line_is_consumed_without_an_event_or_an_error() {
    const CONTEXTS: [&str; 26] = [
        "",
        "---\n",
        "a:\n",
        "a: !t\n",
        "a: &x\n",
        "- a\n",
        "---\n!t\n",
        "---\n&a\n",
        "---\n#c\n",
        "---\n\n",
        "--- # c\n",
        "-\n",
        "?\n",
        "? a\n",
        "? a\n:\n",
        "a: |\n  x\n",
        "x\n",
        "---\n---\n",
        "a: 1\n...\n",
        "---\nx\n...\n",
        "a:\n  b:\n",
        "a:\n  - b\n",
        "--- !t\n",
        "- !t\n",
        "- &a\n",
        "...\n---\n",
    ];
    const FIRST_CHARS: [char; 30] = [
        '%', '@', ',', ']', '}', '#', '!', '&', '*', '|', '>', '\'', '"', '-', '?', ':', '[', '{',
        '.', 'z', '\u{0}', '\u{7}', '\u{1f}', '\u{7f}', '\u{feff}', '\u{85}', '\u{a0}', '\u{2028}',
        '\u{200b}', '\u{3000}',
    ];
    const TAILS: [&str; 3] = ["zz\n", "zz: v\n", "zz\nb: 1\n"];
    for context in CONTEXTS {
        for indent in ["", "  "] {
            for first in FIRST_CHARS {
                for tail in TAILS {
                    let input = format!("{context}{indent}{first}{tail}");
                    let events = parse_to_vec(&input);
                    let carries_text = events.iter().any(|r| match r {
                        Ok((event, _)) => format!("{event:?}").contains("zz"),
                        Err(_) => true,
                    });
                    assert!(carries_text, "input: {input:?}");
                }
            }
        }
    }
}

// An error ends the iterator, so a consumer that does not stop at the first
// error still terminates.
#[test]
fn error_from_a_stray_line_ends_the_stream() {
    let input = "---\n \u{0}x\n".repeat(1000);
    let mut events = parse_events(&input);
    assert!(events.any(|r| r.is_err()));
    assert!(events.next().is_none());
}
