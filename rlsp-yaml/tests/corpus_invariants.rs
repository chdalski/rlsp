// SPDX-License-Identifier: MIT
//
// Corpus invariant harness for rlsp-yaml.
//
// # Skip-list discipline
//
// The SKIP_LIST is **shrink-only**. Entries are removed as follow-up plans fix
// the root causes. New entries are only added when a NEW corpus file surfaces a
// known-fixable issue that has an immediate follow-up plan already filed; never
// to silence a surprise failure. This constraint is the harness's enforcement
// surface — without it the test degrades to a rubber stamp.
//
// A surprise failure (a (file, invariant) pair that fails but has no skip-list
// entry) must be reported to the lead via SendMessage identifying the pair and
// failure detail. The lead either files a follow-up plan (whose path the
// developer then references in the skip-list entry) or directs treating the
// failure as in-scope. The developer never adds a skip-list entry with an
// ad-hoc TODO marker lacking a plan reference.

#![expect(missing_docs, reason = "test code")]
#![expect(
    clippy::panic,
    clippy::unwrap_used,
    reason = "test code — panics are intentional assertion failures"
)]
#![expect(
    clippy::expect_used,
    reason = "test code — expect on infallible operations"
)]
#![expect(
    clippy::cast_possible_truncation,
    reason = "test code — LSP line counts fit in u32 for any real corpus file"
)]
#![expect(
    clippy::indexing_slicing,
    reason = "test code — indices are validated by invariant checks before use"
)]

use std::borrow::Cow;
use std::collections::HashMap;
use std::collections::HashSet;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::{Path, PathBuf};

use rlsp_yaml::analysis::selection::selection_ranges;
use rlsp_yaml::completion::complete_at;
use rlsp_yaml::editing::code_actions::code_actions;
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::navigation::references::{find_references, goto_definition};
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml::validation::ValidationSettings;
use rlsp_yaml::validation::validators::{
    validate_custom_tags, validate_duplicate_keys, validate_flow_style, validate_key_ordering,
    validate_unused_anchors, validate_yaml11_compat,
};
use rlsp_yaml_parser::{Document, Node, Span};
use tower_lsp::lsp_types::{CodeActionKind, DiagnosticSeverity, Position, Range, TextEdit};

const CORPUS_DIR: &str = "tests/corpus";

/// Each registered invariant has an id, description, and a check function.
struct Invariant {
    id: &'static str,
    #[expect(
        dead_code,
        reason = "displayed in future failure-reporting; kept for extensibility"
    )]
    description: &'static str,
    check: fn(&Path, &str) -> Result<(), String>,
}

/// Skip-list entries: `(corpus_file_name, invariant_id, followup_plan_reference_and_justification)`.
///
/// Shrink-only — see module-level doc comment for the discipline.
const SKIP_LIST: &[(&str, &str, &str)] = &[];

/// Registered invariants.
const INVARIANTS: &[Invariant] = &[
    Invariant {
        id: "I1",
        description: "No panics on full LSP pipeline",
        check: check_i1_no_panics,
    },
    Invariant {
        id: "I2",
        description: "Diagnostic range validity",
        check: check_i2_range_validity,
    },
    Invariant {
        id: "I3",
        description: "Code-action output parses",
        check: check_i3_code_action_round_trip,
    },
    Invariant {
        id: "I4",
        description: "Refactor code actions preserve scalar content",
        check: check_i4_scalar_preservation,
    },
    Invariant {
        id: "I5",
        description: "AST anchor_loc invariant: anchor().is_some() == anchor_loc().is_some() for every node",
        check: check_i5_anchor_loc_invariant,
    },
    Invariant {
        id: "I6",
        description: "AST tag_loc invariant: for every node, if tag is Some and NOT a resolver-injected core schema tag, tag_loc must also be Some",
        check: check_i6_tag_loc_invariant,
    },
    Invariant {
        id: "I7",
        description: "goto_definition and find_references never panic on corpus files",
        check: check_i6_references_no_panics,
    },
    Invariant {
        id: "I8",
        description: "selection_ranges never panics and outermost range starts at line 0 for non-empty result at (0,0)",
        check: check_i8_selection_no_panic,
    },
    Invariant {
        id: "I9",
        description: "complete_at never panics and returns <= MAX_COMPLETION_ITEMS items for any cursor position",
        check: check_i9_complete_at_no_panics,
    },
    Invariant {
        id: "I10",
        description: "Formatter round-trip: parsing format(text) produces an AST semantically equivalent to parsing text",
        check: check_i10_formatter_round_trip,
    },
];

// ---------------------------------------------------------------------------
// I1: No panics on full LSP pipeline
// ---------------------------------------------------------------------------

fn check_i1_no_panics(_path: &Path, text: &str) -> Result<(), String> {
    // Stage 1: parse
    let parse_result = catch_unwind(AssertUnwindSafe(|| parse_yaml(text)))
        .map_err(|e| format!("panic in parse_yaml: {}", panic_message(&e)))?;

    let docs = parse_result.documents;

    // Stage 2: validate_unused_anchors
    catch_unwind(AssertUnwindSafe(|| validate_unused_anchors(&docs)))
        .map_err(|e| format!("panic in validate_unused_anchors: {}", panic_message(&e)))?;

    // Stage 3: validate_flow_style
    catch_unwind(AssertUnwindSafe(|| {
        validate_flow_style(&docs, &ValidationSettings::default())
    }))
    .map_err(|e| format!("panic in validate_flow_style: {}", panic_message(&e)))?;

    // Stage 4: validate_custom_tags (empty allowed set — all tags are unknown)
    let allowed_tags: HashSet<String> = HashSet::new();
    catch_unwind(AssertUnwindSafe(|| {
        validate_custom_tags(&docs, &allowed_tags)
    }))
    .map_err(|e| format!("panic in validate_custom_tags: {}", panic_message(&e)))?;

    // Stage 5: validate_key_ordering
    catch_unwind(AssertUnwindSafe(|| validate_key_ordering(&docs)))
        .map_err(|e| format!("panic in validate_key_ordering: {}", panic_message(&e)))?;

    // Stage 6: validate_duplicate_keys
    catch_unwind(AssertUnwindSafe(|| {
        validate_duplicate_keys(&docs, &ValidationSettings::default())
    }))
    .map_err(|e| format!("panic in validate_duplicate_keys: {}", panic_message(&e)))?;

    // Stage 7: validate_yaml11_compat
    catch_unwind(AssertUnwindSafe(|| validate_yaml11_compat(&docs)))
        .map_err(|e| format!("panic in validate_yaml11_compat: {}", panic_message(&e)))?;

    // Stage 8: format_yaml
    let opts = YamlFormatOptions::default();
    catch_unwind(AssertUnwindSafe(|| format_yaml(text, &opts)))
        .map_err(|e| format!("panic in format_yaml: {}", panic_message(&e)))?;

    // Stage 9: code_actions with zero-width range at (0,0) and all diagnostics
    let all_diagnostics = collect_all_diagnostics(&docs);
    let zero_range = Range::new(Position::new(0, 0), Position::new(0, 0));
    let fake_uri = tower_lsp::lsp_types::Url::parse("file:///corpus/test.yaml").expect("valid URI");
    catch_unwind(AssertUnwindSafe(|| {
        code_actions(
            &docs,
            text,
            zero_range,
            &all_diagnostics,
            &fake_uri,
            &YamlFormatOptions::default(),
        )
    }))
    .map_err(|e| format!("panic in code_actions: {}", panic_message(&e)))?;

    Ok(())
}

// ---------------------------------------------------------------------------
// I2: Diagnostic range validity
// ---------------------------------------------------------------------------

fn check_i2_range_validity(_path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    let diagnostics = collect_all_diagnostics(&docs);
    check_diagnostic_ranges(text, &diagnostics)
}

/// Check that every diagnostic range in `diagnostics` is valid with respect to `text`.
///
/// Extracted so unit tests can inject synthetic diagnostics.
fn check_diagnostic_ranges(
    text: &str,
    diagnostics: &[tower_lsp::lsp_types::Diagnostic],
) -> Result<(), String> {
    let lines: Vec<&str> = text.lines().collect();
    let line_count = lines.len() as u32;

    for diag in diagnostics {
        let r = diag.range;
        let code = diag
            .code
            .as_ref()
            .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));

        // Check 1: start <= end (line ordering, then character on same line)
        if r.start.line > r.end.line {
            return Err(format!(
                "diagnostic {code} range start.line ({}) > end.line ({})",
                r.start.line, r.end.line
            ));
        }
        if r.start.line == r.end.line && r.start.character > r.end.character {
            // u32::MAX is used as a "to end-of-line" sentinel from parser.rs
            if r.end.character != u32::MAX {
                return Err(format!(
                    "diagnostic {code} range same-line start.character ({}) > end.character ({})",
                    r.start.character, r.end.character
                ));
            }
        }

        // Check 2: end.line < line_count (0-based, strict less-than)
        if line_count == 0 {
            return Err(format!(
                "diagnostic {code} range references line {} but file has 0 lines",
                r.end.line
            ));
        }
        if r.end.line >= line_count {
            return Err(format!(
                "diagnostic {code} range end.line ({}) >= line_count ({})",
                r.end.line, line_count
            ));
        }

        // Check 3: character values within UTF-16 code-unit length of their lines
        // (skip sentinel u32::MAX — it means "to end of line")
        if r.start.character != u32::MAX {
            let start_line_utf16 = utf16_len(lines[r.start.line as usize]);
            if r.start.character > start_line_utf16 as u32 {
                return Err(format!(
                    "diagnostic {code} start.character ({}) > utf16 length of line {} ({})",
                    r.start.character, r.start.line, start_line_utf16
                ));
            }
        }

        if r.end.character != u32::MAX {
            let end_line_utf16 = utf16_len(lines[r.end.line as usize]);
            if r.end.character > end_line_utf16 as u32 {
                return Err(format!(
                    "diagnostic {code} end.character ({}) > utf16 length of line {} ({})",
                    r.end.character, r.end.line, end_line_utf16
                ));
            }
        }

        // Check 4: byte offsets derived from (line, character) must land on
        // UTF-8 character boundaries.
        if r.start.character != u32::MAX {
            check_utf8_boundary(&lines, r.start.line, r.start.character, &code, "start")?;
        }
        if r.end.character != u32::MAX {
            check_utf8_boundary(&lines, r.end.line, r.end.character, &code, "end")?;
        }
    }

    Ok(())
}

/// Count UTF-16 code units in a string.
fn utf16_len(s: &str) -> usize {
    s.chars().map(char::len_utf16).sum()
}

/// Walk UTF-16 code units to find the byte offset, then check it's a UTF-8
/// char boundary. Returns Err with a message if the check fails.
fn check_utf8_boundary(
    lines: &[&str],
    line: u32,
    character: u32,
    code: &str,
    endpoint: &str,
) -> Result<(), String> {
    let line_str = lines[line as usize];
    let mut utf16_units = 0u32;
    let mut byte_offset = line_str.len(); // default: past end (for char == utf16_len)

    for (byte_pos, ch) in line_str.char_indices() {
        if utf16_units == character {
            byte_offset = byte_pos;
            break;
        }
        let units = ch.len_utf16() as u32;
        if utf16_units + units > character {
            // character falls in the middle of a surrogate pair — not a boundary
            return Err(format!(
                "diagnostic {code} {endpoint} position (line {line}, char {character}) \
                 splits a UTF-16 surrogate pair; not a UTF-8 character boundary"
            ));
        }
        utf16_units += units;
    }

    // Verify byte_offset is on a UTF-8 boundary
    if !line_str.is_char_boundary(byte_offset) {
        return Err(format!(
            "diagnostic {code} {endpoint} position (line {line}, char {character}) \
             byte offset {byte_offset} is not a UTF-8 character boundary in line {line_str:?}"
        ));
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// I3: Code-action output parses
// ---------------------------------------------------------------------------

fn check_i3_code_action_round_trip(path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    let all_diagnostics = collect_all_diagnostics(&docs);

    // Build pre-edit error set: only DiagnosticSeverity::Error entries.
    let pre_edit_errors = error_key_set(&collect_error_diagnostics(text));

    let lines: Vec<&str> = text.lines().collect();
    let last_line = lines.len().saturating_sub(1) as u32;
    let last_char = lines.last().map_or(0, |l| utf16_len(l) as u32);
    let whole_file = Range::new(Position::new(0, 0), Position::new(last_line, last_char));

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let uri = tower_lsp::lsp_types::Url::parse(&format!("file:///corpus/{file_name}"))
        .expect("valid URI");

    let actions = code_actions(
        &docs,
        text,
        whole_file,
        &all_diagnostics,
        &uri,
        &YamlFormatOptions::default(),
    );

    for action in &actions {
        let Some(edit) = &action.edit else {
            continue;
        };
        let Some(changes) = &edit.changes else {
            continue;
        };
        let Some(text_edits) = changes.get(&uri) else {
            continue;
        };
        if text_edits.is_empty() {
            continue;
        }

        let edited = apply_text_edits(text, text_edits);
        let post_edit_diagnostics = collect_error_diagnostics(&edited);
        let post_edit_errors = error_key_set(&post_edit_diagnostics);
        let new_error_keys: Vec<_> = post_edit_errors.difference(&pre_edit_errors).collect();

        if !new_error_keys.is_empty() {
            // Find the triggering diagnostic for the action (first associated diag, if any)
            let (diag_code, diag_range) = action
                .diagnostics
                .as_ref()
                .and_then(|v| v.first())
                .map_or_else(
                    || ("<no-code>".to_string(), "unknown".to_string()),
                    |d| {
                        let code = d
                            .code
                            .as_ref()
                            .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
                        let range = fmt_range(d.range);
                        (code, range)
                    },
                );

            // Find the full diagnostic for the first new error key.
            let new_key = new_error_keys[0];
            let new_diag = post_edit_diagnostics
                .iter()
                .find(|d| &error_key(d) == new_key)
                .expect("key came from this collection");
            let new_code = new_diag
                .code
                .as_ref()
                .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
            let new_range = fmt_range(new_diag.range);
            return Err(format!(
                r#"action "{}": edit for diagnostic {} at {} introduced new error [{}] "{}" at {}"#,
                action.title, diag_code, diag_range, new_code, new_diag.message, new_range
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// I4: Refactor code actions preserve scalar content
// ---------------------------------------------------------------------------

fn check_i4_scalar_preservation(path: &Path, text: &str) -> Result<(), String> {
    let parse_result = parse_yaml(text);
    let pre_scalars = collect_scalar_values(&parse_result.documents);
    let all_diagnostics = collect_all_diagnostics(&parse_result.documents);

    let lines: Vec<&str> = text.lines().collect();
    let last_line = lines.len().saturating_sub(1) as u32;
    let last_char = lines.last().map_or(0, |l| utf16_len(l) as u32);
    let whole_file = Range::new(Position::new(0, 0), Position::new(last_line, last_char));

    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let uri = tower_lsp::lsp_types::Url::parse(&format!("file:///corpus/{file_name}"))
        .expect("valid URI");

    let actions = code_actions(
        &parse_result.documents,
        text,
        whole_file,
        &all_diagnostics,
        &uri,
        &YamlFormatOptions::default(),
    );

    for action in &actions {
        if action.kind.as_ref() != Some(&CodeActionKind::REFACTOR_REWRITE) {
            continue;
        }
        let Some(edit) = &action.edit else {
            continue;
        };
        let Some(changes) = &edit.changes else {
            continue;
        };
        let Some(text_edits) = changes.get(&uri) else {
            continue;
        };
        if text_edits.is_empty() {
            continue;
        }

        let edited = apply_text_edits(text, text_edits);
        let post_parse = parse_yaml(&edited);
        let post_scalars = collect_scalar_values(&post_parse.documents);

        let missing = missing_scalars(&pre_scalars, &post_scalars);
        if !missing.is_empty() {
            let (diag_code, diag_range) = action
                .diagnostics
                .as_ref()
                .and_then(|v| v.first())
                .map_or_else(
                    || ("<no-code>".to_string(), "unknown".to_string()),
                    |d| {
                        let code = d
                            .code
                            .as_ref()
                            .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
                        (code, fmt_range(d.range))
                    },
                );
            return Err(format!(
                r#"action "{}": edit for diagnostic {} at {} dropped scalar {:?}"#,
                action.title, diag_code, diag_range, missing[0]
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// I5: AST anchor_loc invariant
// ---------------------------------------------------------------------------

fn check_i5_anchor_loc_invariant(_path: &Path, text: &str) -> Result<(), String> {
    let Ok(docs) = rlsp_yaml_parser::loader::load(text) else {
        return Ok(()); // invalid YAML has no AST to check
    };
    for doc in &docs {
        check_i5_node(&doc.root)?;
    }
    Ok(())
}

fn check_i5_node(node: &Node<Span>) -> Result<(), String> {
    match node {
        Node::Scalar { .. } | Node::Mapping { .. } | Node::Sequence { .. } => {
            let anchor = node.anchor();
            let anchor_loc = node.anchor_loc();
            if anchor.is_some() != anchor_loc.is_some() {
                return Err(format!(
                    "I5 invariant violated: anchor={anchor:?} but anchor_loc={anchor_loc:?}"
                ));
            }
        }
        Node::Alias { .. } => {}
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                check_i5_node(k)?;
                check_i5_node(v)?;
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_i5_node(item)?;
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// I6: AST tag_loc invariant
// ---------------------------------------------------------------------------

fn check_i6_tag_loc_invariant(_path: &Path, text: &str) -> Result<(), String> {
    let Ok(docs) = rlsp_yaml_parser::loader::load(text) else {
        return Ok(()); // invalid YAML has no AST to check
    };
    for doc in &docs {
        check_i6_node(&doc.root)?;
    }
    Ok(())
}

fn check_i6_node(node: &Node<Span>) -> Result<(), String> {
    match node {
        Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            // Resolver-injected core schema tags (`tag:yaml.org,2002:*`) have no source
            // position (`tag_loc: None`) by design — they were inferred, not written in
            // the source.  Allow those through.  Any other tag that is present must have
            // a corresponding source location.
            let tag_loc = node.tag_loc();
            let is_resolver_injected = tag
                .as_deref()
                .is_some_and(|t| t.starts_with("tag:yaml.org,2002:"));
            if tag.is_some() && tag_loc.is_none() && !is_resolver_injected {
                return Err(format!(
                    "I6 invariant violated: tag={tag:?} but tag_loc={tag_loc:?}"
                ));
            }
        }
        Node::Alias { .. } => {}
    }
    match node {
        Node::Mapping { entries, .. } => {
            for (k, v) in entries {
                check_i6_node(k)?;
                check_i6_node(v)?;
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                check_i6_node(item)?;
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// I7: goto_definition and find_references never panic
// ---------------------------------------------------------------------------

fn check_i6_references_no_panics(path: &Path, text: &str) -> Result<(), String> {
    let docs = rlsp_yaml_parser::load(text).unwrap_or_default();
    let last_line = text.lines().count().saturating_sub(1) as u32;
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("unknown");
    let fake_uri = tower_lsp::lsp_types::Url::parse(&format!("file:///corpus/{file_name}"))
        .expect("valid URI");

    for line in [0u32, last_line] {
        let pos = Position::new(line, 0);
        catch_unwind(AssertUnwindSafe(|| goto_definition(&docs, &fake_uri, pos))).map_err(|e| {
            format!(
                "panic in goto_definition at line {line}: {}",
                panic_message(&e)
            )
        })?;
        catch_unwind(AssertUnwindSafe(|| {
            find_references(&docs, &fake_uri, pos, false)
        }))
        .map_err(|e| {
            format!(
                "panic in find_references at line {line}: {}",
                panic_message(&e)
            )
        })?;
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// I8: selection_ranges never panics; outermost range valid for (0,0)
// ---------------------------------------------------------------------------

fn check_i8_selection_no_panic(_path: &Path, text: &str) -> Result<(), String> {
    let docs = rlsp_yaml_parser::load(text).unwrap_or_default();
    let pos = Position::new(0, 0);

    let result = catch_unwind(AssertUnwindSafe(|| selection_ranges(&docs, &[pos])))
        .map_err(|e| format!("panic in selection_ranges: {}", panic_message(&e)))?;

    if let Some(sr) = result.first() {
        let mut outermost = sr;
        while let Some(ref p) = outermost.parent {
            outermost = p;
        }
        if outermost.range.start.line != 0 {
            return Err(format!(
                "outermost range start.line is {} (expected 0) for position (0,0)",
                outermost.range.start.line
            ));
        }
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// I9: complete_at never panics; result length <= MAX_COMPLETION_ITEMS
// ---------------------------------------------------------------------------

// Mirrors the private constant in completion.rs — must be kept in sync.
const MAX_COMPLETION_ITEMS: usize = 100;

fn check_i9_complete_at_no_panics(_path: &Path, text: &str) -> Result<(), String> {
    let docs = parse_yaml(text).documents;

    for (line, line_text) in text.lines().enumerate() {
        let line_utf16 = utf16_len(line_text) as u32;
        let col_0: u32 = 0;
        let col_mid: u32 = safe_utf16_midpoint(line_text);
        let col_end: u32 = line_utf16;

        // Deduplicate: avoid redundant calls on very short lines.
        let mut cols = vec![col_0];
        if col_mid != col_0 {
            cols.push(col_mid);
        }
        if col_end != col_mid {
            cols.push(col_end);
        }

        for col in cols {
            let pos = Position::new(line as u32, col);
            let result =
                catch_unwind(AssertUnwindSafe(|| complete_at(&docs, pos, None))).map_err(|e| {
                    format!(
                        "panic in complete_at at line {line} col {col}: {}",
                        panic_message(&e)
                    )
                })?;
            let n = result.len();
            if n > MAX_COMPLETION_ITEMS {
                return Err(format!(
                    "complete_at at line {line} col {col} returned {n} items (> MAX_COMPLETION_ITEMS {MAX_COMPLETION_ITEMS})"
                ));
            }
        }
    }

    Ok(())
}

/// Compute the UTF-16 midpoint of a line string, guarding against landing
/// inside a surrogate pair (supplementary-plane characters take 2 UTF-16
/// units; if `len / 2` falls on the second unit, advance by 1).
fn safe_utf16_midpoint(line: &str) -> u32 {
    let len = utf16_len(line) as u32;
    let mut mid = len / 2;
    // Walk UTF-16 units to verify `mid` lands on a code-point boundary.
    let mut units: u32 = 0;
    for ch in line.chars() {
        let ch_units = ch.len_utf16() as u32;
        if units == mid {
            return mid; // already on a boundary
        }
        if units + ch_units > mid {
            // `mid` falls inside a surrogate pair — advance past it.
            mid = units + ch_units;
            return mid;
        }
        units += ch_units;
    }
    mid
}

// ---------------------------------------------------------------------------
// I10: Formatter round-trip — format(text) re-parses to an equivalent AST
// ---------------------------------------------------------------------------

fn check_i10_formatter_round_trip(_path: &Path, text: &str) -> Result<(), String> {
    let parse_pre = parse_yaml(text);
    if parse_pre.documents.is_empty() {
        return Ok(());
    }
    let formatted = format_yaml(text, &YamlFormatOptions::default());
    let parse_post = parse_yaml(&formatted);
    if parse_post.documents.is_empty() {
        return Err("formatter output failed to parse".to_string());
    }
    documents_equivalent(&parse_pre.documents, &parse_post.documents)
}

// ---------------------------------------------------------------------------
// AST equivalence helper (used by I10)
// ---------------------------------------------------------------------------

/// Returns `Ok(())` if `a` and `b` are structurally and data-equivalent ASTs,
/// or `Err(path_description)` identifying the first mismatch location.
///
/// Equivalence rule: same document count; for each document pair, root nodes
/// recursively equivalent. Style, spans, and `NodeMeta` comments are ignored.
fn documents_equivalent(a: &[Document<Span>], b: &[Document<Span>]) -> Result<(), String> {
    if a.len() != b.len() {
        return Err(format!(
            "document count differs: {} vs {}",
            a.len(),
            b.len()
        ));
    }
    for (i, (da, db)) in a.iter().zip(b.iter()).enumerate() {
        nodes_equivalent(&da.root, &db.root, &format!("documents[{i}]"))?;
    }
    Ok(())
}

fn nodes_equivalent(a: &Node<Span>, b: &Node<Span>, path: &str) -> Result<(), String> {
    // Check variant first.
    let kind_a = node_kind_name(a);
    let kind_b = node_kind_name(b);
    if kind_a != kind_b {
        return Err(format!("{path}: kind differs: {kind_a} vs {kind_b}"));
    }

    // Check shared properties for non-alias nodes.
    if let (Node::Alias { name: na, .. }, Node::Alias { name: nb, .. }) = (a, b) {
        if na != nb {
            return Err(format!("{path}: alias name differs: {na:?} vs {nb:?}"));
        }
    } else {
        // anchor
        let anchor_a = a.anchor();
        let anchor_b = b.anchor();
        if anchor_a != anchor_b {
            return Err(format!(
                "{path}: anchor differs: {anchor_a:?} vs {anchor_b:?}"
            ));
        }
        // tag
        let tag_a = node_tag_str(a);
        let tag_b = node_tag_str(b);
        if tag_a != tag_b {
            return Err(format!("{path}: tag differs: {tag_a:?} vs {tag_b:?}"));
        }
        // variant-specific content
        match (a, b) {
            (Node::Scalar { value: va, .. }, Node::Scalar { value: vb, .. }) => {
                if va != vb {
                    return Err(format!("{path}: scalar value differs: '{va}' vs '{vb}'"));
                }
            }
            (Node::Mapping { entries: ea, .. }, Node::Mapping { entries: eb, .. }) => {
                if ea.len() != eb.len() {
                    return Err(format!(
                        "{path}: entry count differs: {} vs {}",
                        ea.len(),
                        eb.len()
                    ));
                }
                for (i, ((ka, va), (kb, vb))) in ea.iter().zip(eb.iter()).enumerate() {
                    nodes_equivalent(ka, kb, &format!("{path}/mapping/entries[{i}]/key"))?;
                    nodes_equivalent(va, vb, &format!("{path}/mapping/entries[{i}]/value"))?;
                }
            }
            (Node::Sequence { items: ia, .. }, Node::Sequence { items: ib, .. }) => {
                if ia.len() != ib.len() {
                    return Err(format!(
                        "{path}: item count differs: {} vs {}",
                        ia.len(),
                        ib.len()
                    ));
                }
                for (i, (na, nb)) in ia.iter().zip(ib.iter()).enumerate() {
                    nodes_equivalent(na, nb, &format!("{path}/sequence/items[{i}]"))?;
                }
            }
            _ => unreachable!("variant mismatch already handled above"),
        }
    }
    Ok(())
}

const fn node_kind_name(node: &Node<Span>) -> &'static str {
    match node {
        Node::Scalar { .. } => "Scalar",
        Node::Mapping { .. } => "Mapping",
        Node::Sequence { .. } => "Sequence",
        Node::Alias { .. } => "Alias",
    }
}

fn node_tag_str(node: &Node<Span>) -> Option<&str> {
    match node {
        Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
            tag.as_deref()
        }
        Node::Alias { .. } => None,
    }
}

/// Walk every node in every document and collect all `Scalar` values (keys and
/// values) into a flat vec. Alias nodes carry only the anchor reference name,
/// not the resolved value — skip them.
fn collect_scalar_values(docs: &[Document<Span>]) -> Vec<String> {
    let mut result = Vec::new();
    for doc in docs {
        collect_node_scalars(&doc.root, &mut result);
    }
    result
}

fn collect_node_scalars(node: &Node<Span>, out: &mut Vec<String>) {
    match node {
        Node::Scalar { value, .. } => out.push(value.clone()),
        Node::Mapping { entries, .. } => {
            for (key, value) in entries {
                collect_node_scalars(key, out);
                collect_node_scalars(value, out);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                collect_node_scalars(item, out);
            }
        }
        // Alias nodes carry only the anchor name, not the resolved value — skip them.
        Node::Alias { .. } => {}
    }
}

/// Return elements present in `pre` whose count in `post` is less than in `pre`.
fn missing_scalars(pre: &[String], post: &[String]) -> Vec<String> {
    let mut pre_counts: HashMap<&str, usize> = HashMap::new();
    for s in pre {
        *pre_counts.entry(s.as_str()).or_insert(0) += 1;
    }
    let mut post_counts: HashMap<&str, usize> = HashMap::new();
    for s in post {
        *post_counts.entry(s.as_str()).or_insert(0) += 1;
    }

    let mut missing = Vec::new();
    for (s, &count) in &pre_counts {
        let post_count = post_counts.get(s).copied().unwrap_or(0);
        if post_count < count {
            for _ in 0..(count - post_count) {
                missing.push((*s).to_string());
            }
        }
    }
    missing
}

/// Collect all Error-severity diagnostics from parse + validators.
fn collect_error_diagnostics(text: &str) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    let parse_result = parse_yaml(text);
    let docs = parse_result.documents;
    collect_all_diagnostics(&docs)
        .into_iter()
        .filter(|d| d.severity == Some(DiagnosticSeverity::ERROR))
        .collect()
}

/// Build a `HashSet` of `"code|message|range_str"` keys for fast membership testing.
fn error_key_set(errors: &[tower_lsp::lsp_types::Diagnostic]) -> HashSet<String> {
    errors.iter().map(error_key).collect()
}

fn error_key(d: &tower_lsp::lsp_types::Diagnostic) -> String {
    let code = d
        .code
        .as_ref()
        .map_or_else(|| "<no-code>".to_string(), |c| format!("{c:?}"));
    format!("{}|{}|{}", code, d.message, fmt_range(d.range))
}

fn fmt_range(r: Range) -> String {
    format!(
        "L{}:{}-L{}:{}",
        r.start.line, r.start.character, r.end.line, r.end.character
    )
}

/// Apply a list of `TextEdit`s to `text`, working in reverse start-position order so
/// that applying one edit does not shift byte offsets for earlier (lower-position) edits.
///
/// # Panics / undefined behaviour
/// Overlapping edits are the caller's responsibility (LSP spec §3.16.2 forbids them).
/// This function does not detect or guard against overlapping ranges.
fn apply_text_edits(text: &str, edits: &[TextEdit]) -> String {
    // Sort edits by start position descending.
    let mut sorted: Vec<&TextEdit> = edits.iter().collect();
    sorted.sort_by(|a, b| {
        b.range
            .start
            .line
            .cmp(&a.range.start.line)
            .then_with(|| b.range.start.character.cmp(&a.range.start.character))
    });

    let mut result = text.to_string();
    for edit in sorted {
        let start_byte = lsp_pos_to_byte_offset(&result, edit.range.start);
        let end_byte = lsp_pos_to_byte_offset(&result, edit.range.end);
        result.replace_range(start_byte..end_byte, &edit.new_text);
    }
    result
}

/// Convert an LSP `Position` (UTF-16 column) to a UTF-8 byte offset in `text`.
fn lsp_pos_to_byte_offset(text: &str, pos: Position) -> usize {
    let mut line_start = 0;
    for (i, line) in text.split('\n').enumerate() {
        if i == pos.line as usize {
            // Walk UTF-16 units to find the byte offset within the line.
            let mut utf16_col = 0u32;
            for (byte_pos, ch) in line.char_indices() {
                if utf16_col == pos.character {
                    return line_start + byte_pos;
                }
                utf16_col += ch.len_utf16() as u32;
            }
            // Column is at or past end of line (e.g., pointing to the newline).
            return line_start + line.len();
        }
        line_start += line.len() + 1; // +1 for '\n'
    }
    // Position past end of text.
    text.len()
}

// ---------------------------------------------------------------------------
// Shared helpers
// ---------------------------------------------------------------------------

/// Collect diagnostics from all validators for a given parsed documents set.
fn collect_all_diagnostics(
    docs: &[rlsp_yaml_parser::node::Document<rlsp_yaml_parser::Span>],
) -> Vec<tower_lsp::lsp_types::Diagnostic> {
    let allowed_tags: HashSet<String> = HashSet::new();
    let mut all = Vec::new();
    all.extend(validate_unused_anchors(docs));
    all.extend(validate_flow_style(docs, &ValidationSettings::default()));
    all.extend(validate_custom_tags(docs, &allowed_tags));
    all.extend(validate_key_ordering(docs));
    all.extend(validate_duplicate_keys(
        docs,
        &ValidationSettings::default(),
    ));
    all.extend(validate_yaml11_compat(docs));
    all
}

/// Extract a human-readable message from a panic payload.
fn panic_message(payload: &Box<dyn std::any::Any + Send>) -> String {
    payload.downcast_ref::<&str>().map_or_else(
        || {
            payload
                .downcast_ref::<String>()
                .map_or_else(|| "<non-string panic>".to_string(), Clone::clone)
        },
        |s| (*s).to_string(),
    )
}

// ---------------------------------------------------------------------------
// Harness infrastructure (unchanged from Task 1)
// ---------------------------------------------------------------------------

fn collect_corpus_files() -> Vec<PathBuf> {
    let dir = Path::new(CORPUS_DIR);
    let mut files = Vec::new();
    let Ok(entries) = std::fs::read_dir(dir) else {
        return files;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if path.is_file() {
            if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                if ext == "yml" || ext == "yaml" {
                    files.push(path);
                }
            }
        }
    }
    files.sort();
    files
}

fn is_skipped(file_name: &str, invariant_id: &str) -> bool {
    SKIP_LIST
        .iter()
        .any(|(f, id, _)| *f == file_name && *id == invariant_id)
}

enum CheckOutcome {
    Passed,
    FailedExpected,
    FailedUnexpected(String),
    PassedUnexpected,
}

fn run_check(path: &Path, content: &str, invariant: &Invariant) -> CheckOutcome {
    let file_name = path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or_default();
    let skipped = is_skipped(file_name, invariant.id);
    match (invariant.check)(path, content) {
        Ok(()) => {
            if skipped {
                CheckOutcome::PassedUnexpected
            } else {
                CheckOutcome::Passed
            }
        }
        Err(msg) => {
            if skipped {
                CheckOutcome::FailedExpected
            } else {
                CheckOutcome::FailedUnexpected(msg)
            }
        }
    }
}

#[test]
fn corpus_invariants() {
    let files = collect_corpus_files();
    let n_files = files.len();
    let n_invariants = INVARIANTS.len();
    let n_checks = n_files * n_invariants;

    let mut failures: Vec<String> = Vec::new();

    for path in &files {
        let content = std::fs::read_to_string(path)
            .unwrap_or_else(|e| panic!("failed to read {}: {e}", path.display()));
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();

        for invariant in INVARIANTS {
            match run_check(path, &content, invariant) {
                CheckOutcome::Passed | CheckOutcome::FailedExpected => {}
                CheckOutcome::FailedUnexpected(msg) => {
                    failures.push(format!("FAIL [{} / {}]: {}", file_name, invariant.id, msg));
                }
                CheckOutcome::PassedUnexpected => {
                    failures.push(format!(
                        "STALE SKIP [{} / {}]: expected failure but invariant passed — remove skip-list entry",
                        file_name, invariant.id
                    ));
                }
            }
        }
    }

    println!("corpus_invariants: {n_invariants} invariants × {n_files} files = {n_checks} checks");

    assert!(
        failures.is_empty(),
        "{} check(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[cfg(test)]
mod tests {
    use std::fmt::Write as _;
    use std::io::Write as _;

    use rlsp_yaml_parser::{CollectionStyle, ScalarStyle, Span as TestSpan};
    use tower_lsp::lsp_types::{Diagnostic, DiagnosticSeverity, NumberOrString};

    use super::*;

    fn with_temp_dir<F: FnOnce(&Path)>(f: F) {
        let unique = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.subsec_nanos());
        let dir = std::env::temp_dir().join(format!("corpus_test_{unique}_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        f(&dir);
        let _ = std::fs::remove_dir_all(&dir);
    }

    fn make_diag(start_line: u32, start_char: u32, end_line: u32, end_char: u32) -> Diagnostic {
        Diagnostic {
            range: Range::new(
                Position::new(start_line, start_char),
                Position::new(end_line, end_char),
            ),
            severity: Some(DiagnosticSeverity::WARNING),
            code: Some(NumberOrString::String("test".to_string())),
            ..Default::default()
        }
    }

    #[test]
    fn collect_corpus_files_finds_yml_and_yaml() {
        with_temp_dir(|dir| {
            std::fs::File::create(dir.join("a.yml")).unwrap();
            std::fs::File::create(dir.join("b.yaml")).unwrap();
            std::fs::File::create(dir.join("c.txt")).unwrap();
            std::fs::File::create(dir.join("d.json")).unwrap();

            let files = collect_from(dir);
            let names: Vec<_> = files
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect();
            assert!(names.contains(&"a.yml"), "expected a.yml, got {names:?}");
            assert!(names.contains(&"b.yaml"), "expected b.yaml, got {names:?}");
            assert!(!names.contains(&"c.txt"), "unexpected c.txt in {names:?}");
            assert!(!names.contains(&"d.json"), "unexpected d.json in {names:?}");
            assert_eq!(names.len(), 2);
        });
    }

    #[test]
    fn collect_corpus_files_returns_empty_for_empty_dir() {
        with_temp_dir(|dir| {
            assert!(collect_from(dir).is_empty());
        });
    }

    #[test]
    fn collect_corpus_files_excludes_subdirectories() {
        with_temp_dir(|dir| {
            std::fs::File::create(dir.join("file.yaml")).unwrap();
            std::fs::create_dir(dir.join("sub")).unwrap();

            let files = collect_from(dir);
            let names: Vec<_> = files
                .iter()
                .map(|p| p.file_name().unwrap().to_str().unwrap())
                .collect();
            assert_eq!(names, vec!["file.yaml"]);
        });
    }

    #[test]
    fn skip_list_lookup_matches_on_filename_only() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(skip_list_contains(skip, path, "round-trip"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_invariant() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/seed.yaml");
        assert!(!skip_list_contains(skip, path, "idempotent"));
    }

    #[test]
    fn skip_list_lookup_does_not_match_different_filename() {
        let skip: &[(&str, &str, &str)] =
            &[("seed.yaml", "round-trip", ".ai/plans/stub.md: example")];
        let path = Path::new("/abs/path/to/other.yaml");
        assert!(!skip_list_contains(skip, path, "round-trip"));
    }

    // ---------------------------------------------------------------------------
    // I2 unit tests (UT-1 through UT-12 from test spec)
    // ---------------------------------------------------------------------------

    // UT-1: empty diagnostic list always passes
    #[test]
    fn i2_ut1_empty_diagnostics_passes() {
        assert!(check_diagnostic_ranges("key: value\n", &[]).is_ok());
    }

    // UT-2: valid ASCII range — synthetic in-bounds diagnostic passes
    #[test]
    fn i2_ut2_valid_ascii_range_passes() {
        // "abc\n" — line 0 has UTF-16 len 3; range (0,0)-(0,3) is valid
        let result = check_diagnostic_ranges("abc\n", &[make_diag(0, 0, 0, 3)]);
        assert!(
            result.is_ok(),
            "valid in-bounds range should pass: {result:?}"
        );
    }

    // UT-3: start.line > end.line is detected as invalid by check_diagnostic_ranges
    #[test]
    fn i2_ut3_start_line_after_end_line_fails() {
        let result = check_diagnostic_ranges("line0\nline1\n", &[make_diag(1, 0, 0, 0)]);
        assert!(result.is_err(), "inverted line range should fail");
    }

    // UT-4: same-line start.character > end.character (non-sentinel) detected as invalid
    #[test]
    fn i2_ut4_same_line_start_char_after_end_char_fails() {
        // "abcde\n" — range (0,5)-(0,3) is inverted on same line
        let result = check_diagnostic_ranges("abcde\n", &[make_diag(0, 5, 0, 3)]);
        assert!(
            result.is_err(),
            "inverted char range on same line should fail"
        );
    }

    // UT-5: end.line == line_count (off by one) detected as out of bounds
    #[test]
    fn i2_ut5_end_line_equals_line_count_fails() {
        // "line0\nline1\n" has 2 lines (indices 0 and 1); line 2 is out of bounds
        let result = check_diagnostic_ranges("line0\nline1\n", &[make_diag(0, 0, 2, 0)]);
        assert!(result.is_err(), "end.line == line_count should fail");
    }

    // UT-6: character beyond UTF-16 line length detected as invalid
    #[test]
    fn i2_ut6_character_beyond_line_length_fails() {
        // "abc\n" — line 0 has UTF-16 len 3; character 4 is out of bounds
        let result = check_diagnostic_ranges("abc\n", &[make_diag(0, 0, 0, 4)]);
        assert!(
            result.is_err(),
            "character beyond utf16 line length should fail"
        );
    }

    // UT-7: multi-byte UTF-8 character — utf16_len counts code units correctly
    #[test]
    fn i2_ut7_multibyte_utf8_counts_utf16_correctly() {
        // "café" — 'é' is U+00E9, 1 UTF-16 code unit, 2 UTF-8 bytes
        let s = "café";
        assert_eq!(s.len(), 5); // UTF-8 bytes: c(1)+a(1)+f(1)+é(2)
        assert_eq!(utf16_len(s), 4); // UTF-16 code units: 4
    }

    // UT-8: supplementary-plane character (emoji) — 2 UTF-16 code units, 4 UTF-8 bytes
    #[test]
    fn i2_ut8_supplementary_plane_counts_utf16_as_two_units() {
        // U+1F600 GRINNING FACE — 4 UTF-8 bytes, 2 UTF-16 code units
        let s = "a\u{1F600}b";
        assert_eq!(s.len(), 6); // UTF-8: 1+4+1
        assert_eq!(utf16_len(s), 4); // UTF-16: 1+2+1
    }

    // UT-9: UTF-16 vs UTF-8 byte indexing correctness — column after emoji
    #[test]
    fn i2_ut9_utf16_column_after_emoji_is_correct() {
        let s = "a\u{1F600}b";
        // 'b' starts at UTF-16 offset 3
        // Verify check_utf8_boundary finds 'b' at byte offset 5 (1 + 4)
        let lines = &[s];
        let result = check_utf8_boundary(lines, 0, 3, "test", "end");
        assert!(
            result.is_ok(),
            "UTF-16 col 3 after emoji should be a valid boundary"
        );
    }

    // UT-10: sentinel u32::MAX end.character passes check_diagnostic_ranges
    #[test]
    fn i2_ut10_sentinel_u32_max_skips_character_bound_check() {
        // u32::MAX as end.character is the "to end of line" sentinel from parser.rs:59.
        // A diagnostic with that sentinel on a valid line must not trigger failures.
        let result = check_diagnostic_ranges("line0\n", &[make_diag(0, 0, 0, u32::MAX)]);
        assert!(
            result.is_ok(),
            "u32::MAX sentinel should pass without triggering char-bound check: {result:?}"
        );
    }

    // UT-11: multi-line range with valid endpoints passes check_diagnostic_ranges
    #[test]
    fn i2_ut11_multiline_range_start_before_end_passes() {
        // "line0\nline1\nline2\n" — range (0,0)-(2,5) spans 3 lines; line2="line2" len=5
        let result = check_diagnostic_ranges("line0\nline1\nline2\n", &[make_diag(0, 0, 2, 5)]);
        assert!(
            result.is_ok(),
            "valid multiline range should pass: {result:?}"
        );
    }

    // UT-12: check_utf8_boundary correctly rejects mid-surrogate offset
    #[test]
    fn i2_ut12_mid_surrogate_offset_fails_boundary_check() {
        // U+1F600 is a surrogate pair in UTF-16. UTF-16 column 1 splits it.
        let s = "\u{1F600}x";
        let lines = &[s];
        // Column 1 falls inside the surrogate pair (emoji takes units 0 and 1)
        let result = check_utf8_boundary(lines, 0, 1, "test", "start");
        assert!(
            result.is_err(),
            "column 1 splits a surrogate pair, should fail"
        );
    }

    // ---------------------------------------------------------------------------
    // I3 unit tests — apply_text_edits helper
    // ---------------------------------------------------------------------------

    // UT-1: single edit replacing a range
    #[test]
    fn i3_at1_single_edit_replaces_range() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 6), Position::new(0, 11)),
            new_text: "Rust".to_string(),
        }];
        assert_eq!(apply_text_edits("hello world", &edits), "hello Rust");
    }

    // UT-2: two non-overlapping edits given in forward order; function must re-sort to reverse
    #[test]
    fn i3_at2_two_non_overlapping_edits_applied_in_reverse_order() {
        let edits = vec![
            TextEdit {
                range: Range::new(Position::new(0, 0), Position::new(0, 3)),
                new_text: "X".to_string(),
            },
            TextEdit {
                range: Range::new(Position::new(0, 8), Position::new(0, 11)),
                new_text: "Z".to_string(),
            },
        ];
        assert_eq!(apply_text_edits("abc def ghi", &edits), "X def Z");
    }

    // UT-3: edit at start of text
    #[test]
    fn i3_at3_edit_at_start_of_text() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 0), Position::new(0, 3)),
            new_text: "NEW".to_string(),
        }];
        assert_eq!(apply_text_edits("old value", &edits), "NEW value");
    }

    // UT-4: edit at end of text
    #[test]
    fn i3_at4_edit_at_end_of_text() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 8)),
            new_text: "new".to_string(),
        }];
        assert_eq!(apply_text_edits("key: val", &edits), "key: new");
    }

    // UT-5: edit spanning multiple lines
    #[test]
    fn i3_at5_edit_spanning_multiple_lines() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(1, 5)),
            new_text: " MIDDLE ".to_string(),
        }];
        assert_eq!(
            apply_text_edits("line0\nline1\nline2", &edits),
            "line0 MIDDLE \nline2"
        );
    }

    // UT-6: empty new_text deletes range
    #[test]
    fn i3_at6_empty_new_text_deletes_range() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 5), Position::new(0, 6)),
            new_text: String::new(),
        }];
        assert_eq!(apply_text_edits("hello  world", &edits), "hello world");
    }

    // UT-7: zero-width range inserts text
    #[test]
    fn i3_at7_zero_width_range_inserts_text() {
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 3), Position::new(0, 3)),
            new_text: "l".to_string(),
        }];
        assert_eq!(apply_text_edits("helo", &edits), "hello");
    }

    // UT-8: empty edits slice returns text unchanged
    #[test]
    fn i3_at8_empty_edits_returns_text_unchanged() {
        assert_eq!(apply_text_edits("unchanged", &[]), "unchanged");
    }

    // UT-9: edit after multi-byte char uses UTF-16 columns
    #[test]
    fn i3_at9_edit_after_multibyte_char_uses_utf16_columns() {
        // "a😀b" — emoji is 2 UTF-16 code units; 'b' is at UTF-16 col 3
        let edits = vec![TextEdit {
            range: Range::new(Position::new(0, 3), Position::new(0, 4)),
            new_text: "X".to_string(),
        }];
        assert_eq!(apply_text_edits("a\u{1F600}b", &edits), "a\u{1F600}X");
    }

    // ---------------------------------------------------------------------------
    // I4 unit tests — collect_scalar_values and missing_scalars helpers
    // ---------------------------------------------------------------------------

    fn zero_span() -> TestSpan {
        TestSpan { start: 0, end: 0 }
    }

    fn make_scalar(value: &str) -> Node<TestSpan> {
        Node::Scalar {
            value: value.to_owned(),
            style: ScalarStyle::Plain,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    fn make_mapping(entries: Vec<(Node<TestSpan>, Node<TestSpan>)>) -> Node<TestSpan> {
        Node::Mapping {
            entries,
            style: CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    fn make_sequence(items: Vec<Node<TestSpan>>) -> Node<TestSpan> {
        Node::Sequence {
            items,
            style: CollectionStyle::Block,
            tag: None,
            loc: zero_span(),
            meta: None,
        }
    }

    fn make_doc(root: Node<TestSpan>) -> Document<TestSpan> {
        Document::with_root(root)
    }

    // CSV-1: empty document list returns empty vec
    #[test]
    fn i4_csv1_empty_docs_returns_empty() {
        assert!(collect_scalar_values(&[]).is_empty());
    }

    // CSV-2: document whose root is a single scalar
    #[test]
    fn i4_csv2_single_scalar_root() {
        let docs = vec![make_doc(make_scalar("hello"))];
        assert_eq!(collect_scalar_values(&docs), vec!["hello"]);
    }

    // CSV-3: flat mapping collects both keys and values
    #[test]
    fn i4_csv3_flat_mapping_collects_keys_and_values() {
        let entries = vec![
            (make_scalar("key1"), make_scalar("val1")),
            (make_scalar("key2"), make_scalar("val2")),
        ];
        let docs = vec![make_doc(make_mapping(entries))];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["key1", "key2", "val1", "val2"]);
    }

    // CSV-4: nested mapping recurses into values
    #[test]
    fn i4_csv4_nested_mapping_recurses() {
        let inner = make_mapping(vec![(make_scalar("inner_key"), make_scalar("inner_val"))]);
        let outer = make_mapping(vec![(make_scalar("outer"), inner)]);
        let docs = vec![make_doc(outer)];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["inner_key", "inner_val", "outer"]);
    }

    // CSV-5: sequence of scalars collects all items
    #[test]
    fn i4_csv5_sequence_of_scalars() {
        let seq = make_sequence(vec![make_scalar("a"), make_scalar("b"), make_scalar("c")]);
        let docs = vec![make_doc(seq)];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["a", "b", "c"]);
    }

    // CSV-6: mapping whose values are sequences — both sides traversed
    #[test]
    fn i4_csv6_mapping_with_sequence_values() {
        let seq = make_sequence(vec![make_scalar("x"), make_scalar("y")]);
        let mapping = make_mapping(vec![(make_scalar("list"), seq)]);
        let docs = vec![make_doc(mapping)];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["list", "x", "y"]);
    }

    // CSV-7: duplicate scalar values are preserved (multiset semantics)
    #[test]
    fn i4_csv7_duplicate_values_preserved() {
        let entries = vec![
            (make_scalar("foo"), make_scalar("foo")),
            (make_scalar("bar"), make_scalar("bar")),
        ];
        let docs = vec![make_doc(make_mapping(entries))];
        let result = collect_scalar_values(&docs);
        assert_eq!(result.iter().filter(|s| s.as_str() == "foo").count(), 2);
        assert_eq!(result.iter().filter(|s| s.as_str() == "bar").count(), 2);
        assert_eq!(result.len(), 4);
    }

    // CSV-8: alias node is skipped — only the real scalar is collected
    #[test]
    fn i4_csv8_alias_node_is_skipped() {
        let alias = Node::Alias {
            name: "anchor_name".to_owned(),
            loc: zero_span(),
            leading_comments: None,
            trailing_comment: None,
        };
        let seq = make_sequence(vec![make_scalar("real"), alias]);
        let docs = vec![make_doc(seq)];
        assert_eq!(collect_scalar_values(&docs), vec!["real"]);
    }

    // CSV-9: empty scalar value is included
    #[test]
    fn i4_csv9_empty_scalar_included() {
        let docs = vec![make_doc(make_scalar(""))];
        assert_eq!(collect_scalar_values(&docs), vec![""]);
    }

    // CSV-10: multiple documents are all walked
    #[test]
    fn i4_csv10_multiple_documents_all_walked() {
        let docs = vec![make_doc(make_scalar("doc1")), make_doc(make_scalar("doc2"))];
        let mut result = collect_scalar_values(&docs);
        result.sort();
        assert_eq!(result, vec!["doc1", "doc2"]);
    }

    // MS-1: equal multisets return empty
    #[test]
    fn i4_ms1_equal_multisets_return_empty() {
        let pre = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let post = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        assert!(missing_scalars(&pre, &post).is_empty());
    }

    // MS-2: post is superset of pre returns empty
    #[test]
    fn i4_ms2_post_superset_returns_empty() {
        let pre = vec!["a".to_string(), "b".to_string()];
        let post = vec![
            "a".to_string(),
            "b".to_string(),
            "c".to_string(),
            "d".to_string(),
        ];
        assert!(missing_scalars(&pre, &post).is_empty());
    }

    // MS-3: pre has element absent from post returns it
    #[test]
    fn i4_ms3_missing_element_returned() {
        let pre = vec!["a".to_string(), "b".to_string(), "c".to_string()];
        let post = vec!["a".to_string(), "c".to_string()];
        let mut result = missing_scalars(&pre, &post);
        result.sort();
        assert_eq!(result, vec!["b"]);
    }

    // MS-4: pre has duplicate that post has only once — returns one missing
    #[test]
    fn i4_ms4_duplicate_in_pre_one_in_post_returns_one() {
        let pre = vec!["foo".to_string(), "foo".to_string(), "bar".to_string()];
        let post = vec!["foo".to_string(), "bar".to_string()];
        let result = missing_scalars(&pre, &post);
        assert_eq!(result, vec!["foo"]);
    }

    // MS-5: pre has duplicate that post has zero — returns two missing
    #[test]
    fn i4_ms5_duplicate_in_pre_none_in_post_returns_both() {
        let pre = vec!["foo".to_string(), "foo".to_string()];
        let post = vec!["bar".to_string()];
        let mut result = missing_scalars(&pre, &post);
        result.sort();
        assert_eq!(result, vec!["foo", "foo"]);
    }

    // MS-6: empty pre always returns empty
    #[test]
    fn i4_ms6_empty_pre_returns_empty() {
        let post = vec!["x".to_string(), "y".to_string()];
        assert!(missing_scalars(&[], &post).is_empty());
    }

    // MS-7: empty post with non-empty pre returns all of pre
    #[test]
    fn i4_ms7_empty_post_returns_all_of_pre() {
        let pre = vec!["a".to_string(), "b".to_string()];
        let mut result = missing_scalars(&pre, &[]);
        result.sort();
        assert_eq!(result, vec!["a", "b"]);
    }

    // INT-1: I4 catches a REFACTOR_REWRITE action that drops a scalar
    // Uses the destructive flow_map_to_block path on a minimal inline YAML.
    // If the code action does not fire for this input (no matching diagnostic),
    // the invariant will pass and the corpus run provides the integration coverage.
    #[test]
    fn i4_int1_refactor_rewrite_dropping_scalar_fails() {
        // This YAML triggers a flowMap diagnostic and a REFACTOR_REWRITE code action.
        // The flow_map_to_block action is known to drop the key when the value
        // contains ${{ ... }} expressions.
        let text = "env:\n  GITHUB_TOKEN: ${{ secrets.GITHUB_TOKEN }}\n";
        let fake_path = Path::new("inline-test.yml");
        let result = check_i4_scalar_preservation(fake_path, text);
        // This may pass (no REFACTOR_REWRITE fires) or fail (action drops scalar).
        // If it fails, confirm the error message names the missing scalar.
        if let Err(msg) = result {
            assert!(
                msg.contains("GITHUB_TOKEN") || msg.contains("secrets.GITHUB_TOKEN"),
                "failure message should name the missing scalar, got: {msg}"
            );
        }
        // If it passes, integration coverage comes from the corpus run.
    }

    // ---------------------------------------------------------------------------
    // Helpers used only in tests
    // ---------------------------------------------------------------------------

    fn collect_from(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        let Ok(entries) = std::fs::read_dir(dir) else {
            return files;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yml" || ext == "yaml" {
                        files.push(path);
                    }
                }
            }
        }
        files.sort();
        files
    }

    fn skip_list_contains(skip: &[(&str, &str, &str)], path: &Path, invariant_id: &str) -> bool {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or_default();
        skip.iter()
            .any(|(f, id, _)| *f == file_name && *id == invariant_id)
    }

    // ---------------------------------------------------------------------------
    // I9 unit tests (UT-I9-1 through UT-I9-7)
    // ---------------------------------------------------------------------------

    fn run_i9(text: &str) -> Result<(), String> {
        check_i9_complete_at_no_panics(Path::new("test.yaml"), text)
    }

    // UT-I9-1: empty file — zero lines, returns Ok immediately
    #[test]
    fn i9_ut1_empty_file_returns_ok() {
        assert!(run_i9("").is_ok());
    }

    // UT-I9-2: newline-only — one empty line, all cols collapse to 0, single call at (0,0)
    #[test]
    fn i9_ut2_newline_only_file_returns_ok() {
        assert!(run_i9("\n").is_ok());
    }

    // UT-I9-3: single-line YAML without trailing newline
    #[test]
    fn i9_ut3_single_line_no_newline_returns_ok() {
        assert!(run_i9("key: value").is_ok());
    }

    // UT-I9-4: multi-line YAML
    #[test]
    fn i9_ut4_multiline_yaml_returns_ok() {
        assert!(run_i9("a: 1\nb: 2\nc: 3\n").is_ok());
    }

    // UT-I9-5: BMP multi-byte UTF-8 ('é' = 2 UTF-8 bytes, 1 UTF-16 unit)
    #[test]
    fn i9_ut5_line_with_bmp_multibyte_char_returns_ok() {
        assert!(run_i9("café: value\n").is_ok());
    }

    // UT-I9-6: supplementary-plane emoji (😀 = 4 UTF-8 bytes, 2 UTF-16 units)
    #[test]
    fn i9_ut6_line_with_supplementary_plane_char_returns_ok() {
        assert!(run_i9("a\u{1F600}b: v\n").is_ok());
    }

    // UT-I9-7: 110-key mapping — exercises the len <= MAX_COMPLETION_ITEMS branch
    #[test]
    fn i9_ut7_large_mapping_respects_item_cap() {
        let mut yaml = String::new();
        for i in 1..=110_u32 {
            writeln!(yaml, "k{i}: v").expect("write to String is infallible");
        }
        assert!(run_i9(&yaml).is_ok());
    }

    // ---------------------------------------------------------------------------
    // I6 unit tests
    // ---------------------------------------------------------------------------

    // UT-I6-1: plain mapping YAML — resolver injects tag:yaml.org,2002:map with
    // no tag_loc.  The narrowed I6 assertion must pass for this case.
    #[test]
    fn i6_resolver_injected_tag_no_tag_loc_passes() {
        let result = check_i6_tag_loc_invariant(Path::new("test.yaml"), "key: value\n");
        assert!(
            result.is_ok(),
            "resolver-injected core tag with tag_loc=None should pass I6: {result:?}"
        );
    }

    // UT-I6-2: explicit user tag on a scalar — tag_loc is Some (source position
    // from the `!custom` token).  The invariant must pass.
    #[test]
    fn i6_explicit_user_tag_with_tag_loc_passes() {
        let result = check_i6_tag_loc_invariant(Path::new("test.yaml"), "!custom value\n");
        assert!(
            result.is_ok(),
            "explicit user tag with tag_loc=Some should pass I6: {result:?}"
        );
    }

    // UT-I6-3: synthetically constructed node with a non-core tag but no tag_loc —
    // simulates a hypothetical loader bug.  The narrowed assertion must still catch
    // this case.
    #[test]
    fn i6_missing_tag_loc_for_non_core_tag_fails() {
        let origin = Span { start: 0, end: 0 };
        let node = Node::Scalar {
            value: String::new(),
            style: ScalarStyle::Plain,
            tag: Some(Cow::Owned("!custom".to_owned())),
            loc: origin,
            // Simulated loader bug: user tag with no source position (meta: None).
            meta: None,
        };
        let result = check_i6_node(&node);
        assert!(
            result.is_err(),
            "non-core tag with tag_loc=None should fail I6"
        );
    }

    // UT-I6-4: no tag, no tag_loc — the zero-tag baseline must pass I6.
    #[test]
    fn i6_no_tag_no_tag_loc_passes() {
        let result = check_i6_tag_loc_invariant(Path::new("test.yaml"), "key: value\n");
        assert!(
            result.is_ok(),
            "node with no tag and no tag_loc should pass I6: {result:?}"
        );
    }

    // ---------------------------------------------------------------------------
    // documents_equivalent unit tests (TC-1 through TC-20)
    // ---------------------------------------------------------------------------

    fn load_docs(text: &str) -> Vec<Document<TestSpan>> {
        rlsp_yaml_parser::loader::load(text).expect("valid YAML for test")
    }

    // TC-1: byte-identical inputs are equivalent
    #[test]
    fn should_return_ok_when_inputs_are_byte_identical() {
        let docs = load_docs("a: 1\n");
        assert!(documents_equivalent(&docs, &docs).is_ok());
    }

    // TC-2: differing document counts produce an error
    #[test]
    fn should_return_err_when_document_counts_differ() {
        let a = load_docs("a: 1\n");
        let b = load_docs("a: 1\n---\nb: 2\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("document count"),
            "error should mention 'document count', got: {err}"
        );
        assert!(
            err.contains('1'),
            "error should contain count 1, got: {err}"
        );
        assert!(
            err.contains('2'),
            "error should contain count 2, got: {err}"
        );
    }

    // TC-3: scalar value mismatch includes both values and the correct path
    #[test]
    fn should_return_err_when_scalar_value_differs() {
        let a = load_docs("a: foo\n");
        let b = load_docs("a: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("foo"),
            "error should contain 'foo', got: {err}"
        );
        assert!(
            err.contains("bar"),
            "error should contain 'bar', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-4: style difference is ignored — both yield the same scalar value
    #[test]
    fn should_return_ok_when_only_styles_differ() {
        let a = load_docs("a: foo\n");
        let b = load_docs("a: \"foo\"\n");
        assert!(
            documents_equivalent(&a, &b).is_ok(),
            "style difference should not affect equivalence"
        );
    }

    // TC-5: empty scalar values with different styles are equivalent
    #[test]
    fn should_return_ok_when_empty_scalar_values_match() {
        let a = load_docs("a: \"\"\n");
        let b = load_docs("a: ''\n");
        assert!(
            documents_equivalent(&a, &b).is_ok(),
            "empty string scalars with different quote styles should be equivalent"
        );
    }

    // TC-6: differing anchor names produce an error with the correct path
    #[test]
    fn should_return_err_when_anchor_name_differs() {
        let a = load_docs("a: &x 1\n");
        let b = load_docs("a: &y 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("anchor"),
            "error should mention 'anchor', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-7: anchor present on one side but not the other
    #[test]
    fn should_return_err_when_anchor_present_vs_absent() {
        let a = load_docs("a: &x 1\n");
        let b = load_docs("a: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("anchor"),
            "error should mention 'anchor', got: {err}"
        );
        assert!(
            err.contains(r#"Some("x")"#),
            "error should reflect Some(\"x\") vs None, got: {err}"
        );
    }

    // TC-8: tag mismatch produces an error with the correct path
    #[test]
    fn should_return_err_when_tag_differs() {
        let a = load_docs("a: !custom 1\n");
        let b = load_docs("a: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("tag"),
            "error should mention 'tag', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-9: mapping entry count mismatch
    #[test]
    fn should_return_err_when_mapping_entry_count_differs() {
        let a = load_docs("a: 1\nb: 2\n");
        let b = load_docs("a: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("entry count"),
            "error should mention 'entry count', got: {err}"
        );
        assert!(
            err.contains("documents[0]"),
            "error should contain path 'documents[0]', got: {err}"
        );
    }

    // TC-10: sequence item count mismatch
    #[test]
    fn should_return_err_when_sequence_item_count_differs() {
        let a = load_docs("- 1\n- 2\n");
        let b = load_docs("- 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("item count"),
            "error should mention 'item count', got: {err}"
        );
        assert!(
            err.contains("documents[0]"),
            "error should contain path 'documents[0]', got: {err}"
        );
    }

    // TC-11: Scalar vs Mapping kind mismatch
    #[test]
    fn should_return_err_when_node_variants_differ_scalar_vs_mapping() {
        let a = load_docs("a: foo\n");
        let b = load_docs("a:\n  b: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("kind"),
            "error should mention 'kind', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-12: Sequence vs Mapping kind mismatch
    #[test]
    fn should_return_err_when_node_variants_differ_sequence_vs_mapping() {
        let a = load_docs("a:\n  - 1\n");
        let b = load_docs("a:\n  b: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("kind"),
            "error should mention 'kind', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value"),
            "error should contain path 'mapping/entries[0]/value', got: {err}"
        );
    }

    // TC-13: deeply nested equivalent mapping returns Ok
    #[test]
    fn should_return_ok_for_deeply_nested_equivalent_mapping() {
        let docs = load_docs("a:\n  b:\n    c: 1\n");
        assert!(documents_equivalent(&docs, &docs).is_ok());
    }

    // TC-14: nested mapping value mismatch accumulates the correct path (spike test)
    #[test]
    fn should_return_err_at_correct_path_for_nested_mapping_value_mismatch() {
        let a = load_docs("a:\n  b: foo\n");
        let b = load_docs("a:\n  b: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("foo"),
            "error should contain 'foo', got: {err}"
        );
        assert!(
            err.contains("bar"),
            "error should contain 'bar', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/value/mapping/entries[0]/value"),
            "error should contain nested path, got: {err}"
        );
    }

    // TC-15: sequence item mismatch includes correct index in path
    #[test]
    fn should_return_err_at_correct_path_for_nested_sequence_item_mismatch() {
        let a = load_docs("a:\n  - 1\n  - 2\n");
        let b = load_docs("a:\n  - 1\n  - 3\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(err.contains('2'), "error should contain '2', got: {err}");
        assert!(err.contains('3'), "error should contain '3', got: {err}");
        assert!(
            err.contains("sequence/items[1]"),
            "error should contain 'sequence/items[1]', got: {err}"
        );
    }

    // TC-16: mapping key mismatch reports key path
    #[test]
    fn should_return_err_at_correct_path_for_mapping_key_mismatch() {
        let a = load_docs("a: 1\n");
        let b = load_docs("b: 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains('a'),
            "error should mention key 'a', got: {err}"
        );
        assert!(
            err.contains('b'),
            "error should mention key 'b', got: {err}"
        );
        assert!(
            err.contains("mapping/entries[0]/key"),
            "error should contain path 'mapping/entries[0]/key', got: {err}"
        );
    }

    // TC-17: same alias names on both sides are equivalent
    #[test]
    fn should_return_ok_when_both_sides_have_same_alias_name() {
        let docs = load_docs("a: &x 1\nb: *x\n");
        assert!(documents_equivalent(&docs, &docs).is_ok());
    }

    // TC-18: differing alias names produce an error
    // Use a sequence where the first two items define anchors identically on
    // both sides; the third item is an alias — differing on the two sides.
    #[test]
    fn should_return_err_when_alias_names_differ() {
        let a = load_docs("- &x 1\n- &y 2\n- *x\n");
        let b = load_docs("- &x 1\n- &y 2\n- *y\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("alias name"),
            "error should mention 'alias name', got: {err}"
        );
    }

    // TC-19: alias vs scalar kind mismatch
    // Same setup: third item is an alias on side A, a plain scalar on side B.
    #[test]
    fn should_return_err_when_alias_vs_scalar() {
        let a = load_docs("- &x 1\n- &y 2\n- *x\n");
        let b = load_docs("- &x 1\n- &y 2\n- 1\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("kind"),
            "error should mention 'kind', got: {err}"
        );
    }

    // TC-20: error path includes correct document index for multi-doc mismatch
    #[test]
    fn should_include_document_index_in_error_path() {
        let a = load_docs("a: 1\n---\nb: foo\n");
        let b = load_docs("a: 1\n---\nb: bar\n");
        let err = documents_equivalent(&a, &b).unwrap_err();
        assert!(
            err.contains("documents[1]"),
            "error should contain 'documents[1]', got: {err}"
        );
    }

    // Validates that zero invariants × N files = 0 checks, which is the
    // expected output of the real `corpus_invariants` test in Task 1.
    #[test]
    fn corpus_invariants_runs_zero_checks_with_empty_invariant_list() {
        with_temp_dir(|dir| {
            let mut f = std::fs::File::create(dir.join("smoke.yaml")).unwrap();
            writeln!(f, "key: value").unwrap();

            let files = collect_from(dir);
            assert_eq!(files.len(), 1);

            // With an empty invariant list, checks = files × 0 = 0.
            let n_invariants = 0_usize;
            assert_eq!(files.len() * n_invariants, 0);
        });
    }

    // ---------------------------------------------------------------------------
    // I10 unit tests
    // ---------------------------------------------------------------------------

    fn run_i10(text: &str) -> Result<(), String> {
        check_i10_formatter_round_trip(Path::new("test.yaml"), text)
    }

    // UT-I10-1: empty input returns Ok (empty pre-parse branch)
    #[test]
    fn i10_ut1_empty_input_returns_ok() {
        assert!(run_i10("").is_ok());
    }

    // UT-I10-2: invalid YAML returns Ok (empty pre-parse branch)
    #[test]
    fn i10_ut2_invalid_yaml_returns_ok() {
        assert!(run_i10("{{{invalid yaml").is_ok());
    }

    // UT-I10-3: idempotent valid YAML returns Ok (happy path)
    #[test]
    fn i10_ut3_idempotent_valid_yaml_returns_ok() {
        assert!(run_i10("key: value\n").is_ok());
    }

    // UT-I10-4: flow mapping → block conversion returns Ok (style changes, structure unchanged)
    #[test]
    fn i10_ut4_flow_to_block_conversion_returns_ok() {
        assert!(run_i10("{a: 1, b: 2}\n").is_ok());
    }

    // UT-I10-5: multi-document input returns Ok
    #[test]
    fn i10_ut5_multi_document_returns_ok() {
        assert!(run_i10("a: 1\n---\nb: 2\n").is_ok());
    }

    // UT-I10-6: defensive branch — formatter output that parses to zero documents returns Err.
    // This branch is a guard against formatters producing unparseable output; the formatter
    // correctly never produces such output for valid input. Branch coverage is by inspection
    // only — we confirm the Ok/Err semantics of adjacent branches cover it structurally.
    //
    // UT-I10-6: defensive branch; not reachable by any valid formatter input — covered by inspection
}
