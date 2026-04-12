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
        events.iter().any(|e| matches!(
            e,
            Event::SequenceStart {
                anchor: Some("myseq"),
                tag: Some(t),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:seq"
        )),
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
        events.iter().any(|e| matches!(
            e,
            Event::MappingStart {
                anchor: Some("mymap"),
                tag: Some(t),
                ..
            } if t.as_ref() == "tag:yaml.org,2002:map"
        )),
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
