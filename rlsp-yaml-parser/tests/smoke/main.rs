// SPDX-License-Identifier: MIT
#![deny(clippy::panic)]

//! Smoke / integration tests for `rlsp-yaml-parser`.
//!
//! Tests are grouped by grammar area using nested modules.  Each task adds
//! a new `mod` block here as it introduces new event variants.
//!
//! # Shared helper
//!
//! [`parse_to_vec`] collects the full event stream into a `Vec` without
//! hiding errors.  It is the canonical test helper for all grammar tasks.

use rlsp_yaml_parser::{
    Chomp, CollectionStyle, Error, Event, MAX_ANCHOR_NAME_BYTES, MAX_COLLECTION_DEPTH,
    MAX_COMMENT_LEN, MAX_DIRECTIVES_PER_DOC, MAX_TAG_HANDLE_BYTES, MAX_TAG_LEN, Pos, ScalarStyle,
    Span, parse_events,
};

// ---------------------------------------------------------------------------
// Shared helper for extracting event variants from parse_to_vec
// ---------------------------------------------------------------------------

/// Extract only the `Event` variant (dropping the `Span`) from a `parse_to_vec`
/// result, panicking if any item is an `Err`.
fn event_variants(input: &str) -> Vec<Event<'_>> {
    parse_events(input)
        .map(|r| match r {
            Ok((ev, _span)) => ev,
            Err(e) => unreachable!("unexpected parse error: {e}"),
        })
        .collect()
}

// ---------------------------------------------------------------------------
// Shared helper
// ---------------------------------------------------------------------------

/// Collect `parse_events(input)` into a `Vec`, preserving `Err` items.
///
/// The returned items include `Span`s so that later tasks can assert on
/// event positions.  Tests that only care about variant identity can use
/// `matches!` or extract the event with `.as_ref().unwrap().0`.
fn parse_to_vec(input: &str) -> Vec<Result<(Event<'_>, Span), Error>> {
    parse_events(input).collect()
}

// ---------------------------------------------------------------------------
// Promoted shared helpers (extracted from duplicated local copies)
// ---------------------------------------------------------------------------

/// Parse input and return event variants, panicking on any error.
fn evs(input: &str) -> Vec<Event<'_>> {
    parse_events(input)
        .map(|r| match r {
            Ok((ev, _)) => ev,
            Err(e) => unreachable!("unexpected parse error: {e}"),
        })
        .collect()
}

/// Return `true` if any event in the parse stream is an `Err`.
fn has_error(input: &str) -> bool {
    parse_events(input).any(|r| r.is_err())
}

/// Extract scalar string values from events, skipping non-scalars.
fn scalar_values<'a>(events: &'a [Event<'a>]) -> Vec<&'a str> {
    events
        .iter()
        .filter_map(|e| match e {
            Event::Scalar { value, .. } => Some(value.as_ref()),
            Event::StreamStart
            | Event::StreamEnd
            | Event::DocumentStart { .. }
            | Event::DocumentEnd { .. }
            | Event::SequenceStart { .. }
            | Event::SequenceEnd
            | Event::MappingStart { .. }
            | Event::MappingEnd
            | Event::Alias { .. }
            | Event::Comment { .. } => None,
        })
        .collect()
}

/// Count events matching a predicate.
fn count<'a>(events: &[Event<'a>], pred: impl Fn(&Event<'a>) -> bool) -> usize {
    events.iter().filter(|e| pred(e)).count()
}

// ---------------------------------------------------------------------------
// Submodules
// ---------------------------------------------------------------------------

mod anchors_and_aliases;
mod block_scalars;
mod comments;
mod conformance;
mod directives;
mod documents;
mod flow_collections;
mod folded_scalars;
mod mappings;
mod nested_collections;
mod nested_flow_block_mixing;
mod probe_dispatch;
mod quoted_scalars;
mod scalar_dispatch;
mod scalars;
mod sequences;
mod stream;
mod tags;
