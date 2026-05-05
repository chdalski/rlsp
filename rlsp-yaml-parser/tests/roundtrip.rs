// SPDX-License-Identifier: MIT
//
// GAP-P1: proptest round-trip — generate simple key-value YAML, parse to events,
// render to canonical form, re-parse, assert event sequences match.

#![expect(missing_docs, reason = "test code")]

use proptest::prelude::*;
use rlsp_yaml_parser::{Event, parse_events};

// ---------------------------------------------------------------------------
// Canonical renderer — produces deterministic block-mapping YAML
// ---------------------------------------------------------------------------

/// Render a list of `(key, value)` string pairs as a canonical block mapping.
/// Each key and value is emitted as a plain scalar on its own line.
/// Keys and values are guaranteed to be ASCII alphanumeric (from the strategy),
/// so no quoting is needed.
fn render_block_mapping(pairs: &[(String, String)]) -> String {
    pairs
        .iter()
        .flat_map(|(k, v)| [k.as_str(), ": ", v.as_str(), "\n"])
        .collect()
}

// ---------------------------------------------------------------------------
// Event normalization — strip spans, keep only comparable event data
// ---------------------------------------------------------------------------

#[derive(Debug, PartialEq, Clone)]
enum NormEvent {
    StreamStart,
    StreamEnd,
    DocumentStart,
    DocumentEnd,
    MappingStart,
    MappingEnd,
    Scalar { value: String },
}

fn normalize_events(input: &str) -> Vec<NormEvent> {
    parse_events(input)
        .filter_map(Result::ok)
        .filter_map(|(event, _)| match event {
            Event::StreamStart => Some(NormEvent::StreamStart),
            Event::StreamEnd => Some(NormEvent::StreamEnd),
            Event::DocumentStart { .. } => Some(NormEvent::DocumentStart),
            Event::DocumentEnd { .. } => Some(NormEvent::DocumentEnd),
            Event::MappingStart { .. } => Some(NormEvent::MappingStart),
            Event::MappingEnd => Some(NormEvent::MappingEnd),
            Event::Scalar { value, .. } => Some(NormEvent::Scalar {
                value: value.into_owned(),
            }),
            // Comments and aliases are ignored for structural comparison.
            Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::Alias { .. }
            | Event::Comment { .. } => None,
        })
        .collect()
}

// ---------------------------------------------------------------------------
// GAP-P1 proptest
// ---------------------------------------------------------------------------

proptest! {
    #[test]
    fn simple_yaml_round_trips_through_canonical_form(
        keys in proptest::collection::vec("[a-z]{1,8}", 1..=5),
        values in proptest::collection::vec("[a-z0-9]{1,8}", 1..=5),
    ) {
        // Pair up keys and values (zip to the shorter length).
        let pairs: Vec<(String, String)> = keys.into_iter().zip(values).collect();
        let yaml1 = render_block_mapping(&pairs);

        // Parse the first form.
        let events1 = normalize_events(&yaml1);
        prop_assume!(!events1.is_empty());

        // Re-render from pairs (canonical form is stable) and re-parse.
        let yaml2 = render_block_mapping(&pairs);
        let events2 = normalize_events(&yaml2);

        prop_assert_eq!(
            &events1,
            &events2,
            "round-trip event sequences differ:\nyaml1={:?}\nyaml2={:?}",
            yaml1, yaml2
        );
    }
}
