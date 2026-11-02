// SPDX-License-Identifier: MIT
//
// The formatter loses no comment: for every input that loads, every comment
// event of the input is in the output, the output loads to the same data, and
// formatting it again changes nothing.
//
// The inputs are the cases of the yaml-test-suite, the real files in
// `tests/corpus/`, and the pinned inputs of `EXTRA_INPUTS`.  Each runs as it is
// and in variants with one comment added: ` # tN` after line N, `# oN` on an
// own line before line N at the indentation of that line, and `# fN` on an own
// line before line N at column 0.  A variant counts only when its documents
// are equivalent to the original's and the parser reads the new text as a
// comment; positions inside block scalars and multi-line quoted scalars drop
// out on their own.  A last variant per input holds every accepted comment of
// its kind at once.
//
// Only inputs for which `load()` fails are skipped.  They are the pinned list
// `UNLOADABLE`, which may only shrink; `format_yaml` returns them unchanged.

#![expect(missing_docs, reason = "test code")]

mod inputs;
mod loaded_data;
mod suite_cases;

use std::collections::BTreeSet;
use std::fmt::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::path::Path;
use std::sync::OnceLock;

use inputs::{EXTRA_INPUTS, OptionSet, corpus_files, corpus_source, long_escaped_key, suite_files};
use rlsp_yaml::editing::editor_config::LineEnding;
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::server::YamlVersion;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{Event, ScalarStyle, Span, load, parse_events};
use rstest::rstest;

// ---- inputs -----------------------------------------------------------------

struct Input {
    source: String,
    text: String,
    /// `data_signature` of `text`.
    data: String,
    /// The comment texts of `text`, in order.
    comments: Vec<String>,
}

struct Inputs {
    loaded: Vec<Input>,
    /// `file[index]` of the suite cases that are valid YAML and do not load.
    unloadable: Vec<String>,
    /// The suite cases that the suite marks as errors and that load here.
    invalid_but_loaded: usize,
}

/// The comment texts of `text` as the parser reads them, in order.
fn comment_texts(text: &str) -> Vec<String> {
    parse_events(text)
        .filter_map(|event| match event {
            Ok((Event::Comment { text }, _)) => Some(text.to_string()),
            _ => None,
        })
        .collect()
}

fn inputs() -> &'static Inputs {
    static INPUTS: OnceLock<Inputs> = OnceLock::new();
    INPUTS.get_or_init(|| {
        let mut candidates: Vec<(String, String)> = Vec::new();
        let mut unloadable = Vec::new();
        let mut invalid_but_loaded = 0;
        for path in suite_files() {
            for case in suite_cases::load_cases_from_file(&path) {
                let source = format!("{}[{}] {}", case.file, case.index, case.name);
                if loaded_data::data_signature(&case.yaml).is_some() {
                    invalid_but_loaded += usize::from(case.fail);
                    candidates.push((source, case.yaml));
                } else if !case.fail {
                    unloadable.push(format!("{}[{}]", case.file, case.index));
                }
            }
        }
        for path in corpus_files() {
            let text = std::fs::read_to_string(&path).unwrap_or_default();
            candidates.push((corpus_source(&path), text));
        }
        for (name, text) in EXTRA_INPUTS {
            candidates.push((format!("extra: {name}"), (*text).to_string()));
        }
        candidates.push(("extra: long escaped key".to_string(), long_escaped_key()));
        let loaded = candidates
            .into_iter()
            .filter_map(|(source, text)| {
                let data = loaded_data::data_signature(&text)?;
                let comments = comment_texts(&text);
                Some(Input {
                    source,
                    text,
                    data,
                    comments,
                })
            })
            .collect();
        Inputs {
            loaded,
            unloadable,
            invalid_but_loaded,
        }
    })
}

// ---- variants ---------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Kind {
    /// The input as it is.
    Original,
    /// ` # tN` after line N.
    Trailing,
    /// `# oN` before line N, at the indentation of that line.
    OwnLine,
    /// `# fN` before line N, at column 0.
    OwnLineFlush,
}

/// Floors that keep the variants from vanishing: most positions of most inputs
/// admit a comment, and a run in which they do not has stopped testing the
/// formatter (the generator or the parser changed under it).
const MIN_ADMITTED_PERCENT: usize = 50;
const MIN_INPUTS_WITH_VARIANT_PERCENT: usize = 80;

/// The failures that a sweep shows in its message; a change that breaks one
/// input in every file would fill the screen with the rest.
const MAX_SHOWN_FAILURES: usize = 10;

fn is_blank(line: &str) -> bool {
    line.trim_matches([' ', '\t', '\r', '\n']).is_empty()
}

/// The lines before which or after which `kind` adds a comment: every line that
/// can hold one, since a comment is checked at each position.
fn positions(lines: &[&str], kind: Kind) -> Vec<usize> {
    match kind {
        Kind::Original => Vec::new(),
        Kind::Trailing => lines
            .iter()
            .enumerate()
            .filter(|(_, line)| !is_blank(line) && !line.trim_start().starts_with('#'))
            .map(|(i, _)| i)
            .collect(),
        Kind::OwnLine | Kind::OwnLineFlush => (0..=lines.len()).collect(),
    }
}

/// `text` with the comments of `kind` at `at`, and the comment texts that it
/// adds.
fn with_comments(text: &str, kind: Kind, at: &[usize]) -> (String, Vec<String>) {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let mut out = String::with_capacity(text.len() + at.len() * 8);
    let mut added = Vec::new();
    let own_line = |out: &mut String, added: &mut Vec<String>, i: usize, indent: &str| {
        let (mark, body) = if kind == Kind::OwnLine {
            ('o', indent)
        } else {
            ('f', "")
        };
        let _ = writeln!(out, "{body}# {mark}{i}");
        added.push(format!(" {mark}{i}"));
    };
    for (i, line) in lines.iter().enumerate() {
        if at.contains(&i) {
            match kind {
                Kind::OwnLine | Kind::OwnLineFlush => {
                    let indent_len = line.len() - line.trim_start_matches(' ').len();
                    own_line(&mut out, &mut added, i, &line[..indent_len]);
                }
                Kind::Trailing => {
                    let body = line.trim_end_matches('\n');
                    let (body, cr) = body
                        .strip_suffix('\r')
                        .map_or((body, ""), |rest| (rest, "\r"));
                    let eol = &line[line.trim_end_matches('\n').len()..];
                    let _ = write!(out, "{body} # t{i}{cr}{eol}");
                    added.push(format!(" t{i}"));
                    continue;
                }
                Kind::Original => {}
            }
        }
        out.push_str(line);
    }
    if matches!(kind, Kind::OwnLine | Kind::OwnLineFlush) && at.contains(&lines.len()) {
        if !out.is_empty() && !out.ends_with('\n') {
            out.push('\n');
        }
        own_line(&mut out, &mut added, lines.len(), "");
    }
    (out, added)
}

/// Why a variant does not count.
#[derive(Clone, Copy, Debug)]
enum Rejection {
    NotLoading,
    DataDiffers,
    CommentNotRead,
}

fn sorted(mut texts: Vec<String>) -> Vec<String> {
    texts.sort();
    texts
}

/// A variant counts when it loads to the original's data and its comment texts
/// are the original's and the added ones.
fn admit(base: &Input, variant: &str, added: &[String]) -> Result<(), Rejection> {
    let data = loaded_data::data_signature(variant).ok_or(Rejection::NotLoading)?;
    if data != base.data {
        return Err(Rejection::DataDiffers);
    }
    let mut expected = base.comments.clone();
    expected.extend(added.iter().cloned());
    if sorted(comment_texts(variant)) == sorted(expected) {
        Ok(())
    } else {
        Err(Rejection::CommentNotRead)
    }
}

struct Item {
    source: String,
    text: String,
}

#[derive(Default)]
struct VariantTally {
    generated: usize,
    admitted: usize,
    not_loading: usize,
    data_differs: usize,
    comment_not_read: usize,
    inputs_with_an_admitted_variant: usize,
    combined_rejected: usize,
}

struct Prepared {
    items: Vec<Item>,
    tally: VariantTally,
}

fn prepare(kind: Kind) -> Prepared {
    let mut items = Vec::new();
    let mut tally = VariantTally::default();
    for base in &inputs().loaded {
        if kind == Kind::Original {
            items.push(Item {
                source: base.source.clone(),
                text: base.text.clone(),
            });
            continue;
        }
        let lines: Vec<&str> = base.text.split_inclusive('\n').collect();
        let mut admitted_at = Vec::new();
        for at in positions(&lines, kind) {
            let (text, added) = with_comments(&base.text, kind, &[at]);
            tally.generated += 1;
            match admit(base, &text, &added) {
                Ok(()) => {
                    tally.admitted += 1;
                    admitted_at.push(at);
                    items.push(Item {
                        source: format!("{} + {kind:?} at line {at}", base.source),
                        text,
                    });
                }
                Err(Rejection::NotLoading) => tally.not_loading += 1,
                Err(Rejection::DataDiffers) => tally.data_differs += 1,
                Err(Rejection::CommentNotRead) => tally.comment_not_read += 1,
            }
        }
        tally.inputs_with_an_admitted_variant += usize::from(!admitted_at.is_empty());
        if admitted_at.len() > 1 {
            let (text, added) = with_comments(&base.text, kind, &admitted_at);
            if admit(base, &text, &added).is_ok() {
                items.push(Item {
                    source: format!("{} + {kind:?} at every admitted line", base.source),
                    text,
                });
            } else {
                tally.combined_rejected += 1;
            }
        }
    }
    Prepared { items, tally }
}

fn prepared(kind: Kind) -> &'static Prepared {
    static ORIGINAL: OnceLock<Prepared> = OnceLock::new();
    static TRAILING: OnceLock<Prepared> = OnceLock::new();
    static OWN_LINE: OnceLock<Prepared> = OnceLock::new();
    static OWN_LINE_FLUSH: OnceLock<Prepared> = OnceLock::new();
    let cell = match kind {
        Kind::Original => &ORIGINAL,
        Kind::Trailing => &TRAILING,
        Kind::OwnLine => &OWN_LINE,
        Kind::OwnLineFlush => &OWN_LINE_FLUSH,
    };
    cell.get_or_init(|| prepare(kind))
}

// ---- the check --------------------------------------------------------------

/// The entry of `entries` or its parts defines an anchor.
fn defines_anchor(node: &Node<Span>) -> bool {
    node.anchor().is_some()
        || match node {
            Node::Mapping { entries, .. } => entries
                .iter()
                .any(|(key, value)| defines_anchor(key) || defines_anchor(value)),
            Node::Sequence { items, .. } => items.iter().any(defines_anchor),
            Node::Scalar { .. } | Node::Alias { .. } => false,
        }
}

/// What `format_remove_duplicate_keys` takes as one key: a scalar by tag and
/// value, an alias by name; under YAML 1.1 also by quoting, since `y` is a
/// boolean there and `"y"` a string.  The merge key `<<` and keys that are
/// collections are no keys to merge.
fn duplicate_key_identity(key: &Node<Span>, version: YamlVersion) -> Option<String> {
    match key {
        Node::Scalar {
            value, tag, style, ..
        } => {
            if value == "<<" && *style == ScalarStyle::Plain {
                return None;
            }
            let quoting = if version == YamlVersion::V1_1 {
                format!("{}", *style == ScalarStyle::Plain)
            } else {
                String::new()
            };
            Some(format!("{tag:?}\0{value}\0{quoting}"))
        }
        Node::Alias { name, .. } => Some(format!("*{name}")),
        Node::Mapping { .. } | Node::Sequence { .. } => None,
    }
}

/// The reference for the duplicate keys option, independent of the formatter:
/// of entries with one key the last stays, and so does any entry that defines
/// an anchor, since an alias further down may name it.
fn without_duplicate_keys(node: &mut Node<Span>, version: YamlVersion) {
    match node {
        Node::Mapping { entries, .. } => {
            let identities: Vec<Option<String>> = entries
                .iter()
                .map(|(key, _)| duplicate_key_identity(key, version))
                .collect();
            let keep: Vec<bool> = entries
                .iter()
                .zip(&identities)
                .enumerate()
                .map(|(index, ((key, value), identity))| {
                    identity.is_none()
                        || identities
                            .iter()
                            .skip(index + 1)
                            .all(|later| later != identity)
                        || defines_anchor(key)
                        || defines_anchor(value)
                })
                .collect();
            let mut index = 0;
            entries.retain(|_| {
                index += 1;
                keep.get(index - 1).copied().unwrap_or(true)
            });
            for (_, value) in entries.iter_mut() {
                without_duplicate_keys(value, version);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                without_duplicate_keys(item, version);
            }
        }
        Node::Scalar { .. } | Node::Alias { .. } => {}
    }
}

/// The data that the output of `format_yaml(text, options)` must load to.
fn expected_data(text: &str, options: &YamlFormatOptions) -> String {
    let Ok(mut docs) = load(text) else {
        return String::new();
    };
    let mut out = String::new();
    for doc in &mut docs {
        if options.format_remove_duplicate_keys {
            without_duplicate_keys(&mut doc.root, options.yaml_version);
        }
        out.push_str(&loaded_data::node_signature(&doc.root));
        out.push('\n');
    }
    out
}

enum Problem {
    Panicked,
    DoesNotLoad,
    CommentsDiffer {
        missing: Vec<String>,
        extra: Vec<String>,
    },
    DataDiffers,
    Unstable,
}

impl Problem {
    fn describe(&self) -> String {
        match self {
            Self::Panicked => "format_yaml panics".to_string(),
            Self::DoesNotLoad => "the output does not load".to_string(),
            Self::CommentsDiffer { missing, extra } => {
                format!("the comments differ: missing {missing:?}, extra {extra:?}")
            }
            Self::DataDiffers => "the output loads to other data".to_string(),
            Self::Unstable => "formatting the output changes it".to_string(),
        }
    }
}

/// The elements of `from` that `other` does not hold as often, counting repeats.
fn multiset_difference(from: &[String], other: &[String]) -> Vec<String> {
    let mut available = other.to_vec();
    from.iter()
        .filter(|item| {
            available
                .iter()
                .position(|candidate| candidate == *item)
                .map(|pos| available.swap_remove(pos))
                .is_none()
        })
        .cloned()
        .collect()
}

/// Formats `text`, returning the output and what is wrong with it.
fn check(text: &str, options: &YamlFormatOptions) -> (String, Option<Problem>) {
    let formatted = catch_unwind(AssertUnwindSafe(|| {
        let output = format_yaml(text, options);
        let second = format_yaml(&output, options);
        (output, second)
    }));
    let Ok((output, second)) = formatted else {
        return (String::new(), Some(Problem::Panicked));
    };
    let Some(data) = loaded_data::data_signature(&output) else {
        return (output, Some(Problem::DoesNotLoad));
    };
    let before = comment_texts(text);
    let after = comment_texts(&output);
    let missing = multiset_difference(&before, &after);
    let extra = multiset_difference(&after, &before);
    let problem = if !missing.is_empty() || !extra.is_empty() {
        Some(Problem::CommentsDiffer { missing, extra })
    } else if data != expected_data(text, options) {
        Some(Problem::DataDiffers)
    } else if second != output {
        Some(Problem::Unstable)
    } else {
        None
    };
    (output, problem)
}

/// The first characters of `text`, cut at a character boundary.
fn head(text: &str) -> String {
    let mut shown: String = text.chars().take(240).collect();
    if shown.len() < text.len() {
        shown.push_str("...");
    }
    shown
}

#[rstest]
fn formatter_keeps_every_comment(
    #[values(
        OptionSet::Default,
        OptionSet::EnforceBlockStyle,
        OptionSet::Narrow,
        OptionSet::IndentlessSequences,
        OptionSet::PreserveQuotes,
        OptionSet::SingleQuote,
        OptionSet::RemoveDuplicateKeys,
        OptionSet::TabWidthFour,
        OptionSet::Yaml11,
        OptionSet::Combined
    )]
    option_set: OptionSet,
    #[values(Kind::Original, Kind::Trailing, Kind::OwnLine, Kind::OwnLineFlush)] kind: Kind,
) {
    let options = option_set.options();
    let prepared = prepared(kind);
    let mut failures = Vec::new();
    let mut comments_checked = 0;
    let mut unchanged = 0;
    for item in &prepared.items {
        let (output, problem) = check(&item.text, &options);
        comments_checked += comment_texts(&item.text).len();
        unchanged += usize::from(output == item.text);
        if let Some(problem) = problem {
            failures.push(format!(
                "{}: {}\n  input:  {:?}\n  output: {:?}",
                item.source,
                problem.describe(),
                head(&item.text),
                head(&output),
            ));
        }
    }
    let t = &prepared.tally;
    println!(
        "{option_set:?} / {kind:?}: inputs {}, checked {}, comments {comments_checked}, \
         output equal to input {unchanged}, failures {}; variants generated {}, admitted {} \
         (rejected: not loading {}, data differs {}, comment not read {}, combined {}), \
         inputs with an admitted variant {}",
        inputs().loaded.len(),
        prepared.items.len(),
        failures.len(),
        t.generated,
        t.admitted,
        t.not_loading,
        t.data_differs,
        t.comment_not_read,
        t.combined_rejected,
        t.inputs_with_an_admitted_variant,
    );
    assert!(
        failures.is_empty(),
        "{option_set:?} / {kind:?}: {} of {} checks fail, the first ones:\n{}",
        failures.len(),
        prepared.items.len(),
        failures
            .iter()
            .take(MAX_SHOWN_FAILURES)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n"),
    );
    // A run that checks nothing passes: the variants must reach the formatter.
    assert!(comments_checked > 0, "{option_set:?} / {kind:?}");
    if kind != Kind::Original {
        let loaded = inputs().loaded.len();
        assert!(
            t.admitted * 100 >= t.generated * MIN_ADMITTED_PERCENT,
            "{kind:?}: {} of {} variants admitted",
            t.admitted,
            t.generated
        );
        assert!(
            t.inputs_with_an_admitted_variant * 100 >= loaded * MIN_INPUTS_WITH_VARIANT_PERCENT,
            "{kind:?}: {} of {loaded} inputs have an admitted variant",
            t.inputs_with_an_admitted_variant
        );
    }
}

// ---- the skip list ----------------------------------------------------------

/// The yaml-test-suite cases that are valid YAML and do not load, as
/// `file[index]`.  `format_yaml` returns them unchanged.  The list may only
/// shrink; each entry has its deviation on `.ai/backlog.md`.
const UNLOADABLE: &[&str] = &[];

#[test]
fn the_inputs_that_do_not_load_are_the_pinned_list() {
    let found: BTreeSet<&str> = inputs().unloadable.iter().map(String::as_str).collect();
    let pinned: BTreeSet<&str> = UNLOADABLE.iter().copied().collect();
    assert_eq!(found, pinned);
}

#[test]
fn the_corpus_and_the_pinned_inputs_load() {
    let loaded: BTreeSet<&str> = inputs().loaded.iter().map(|i| i.source.as_str()).collect();
    for path in corpus_files() {
        let source = corpus_source(&path);
        assert!(loaded.contains(source.as_str()), "{source} does not load");
    }
    for (name, _) in EXTRA_INPUTS {
        let source = format!("extra: {name}");
        assert!(loaded.contains(source.as_str()), "{source} does not load");
    }
    assert!(loaded.contains("extra: long escaped key"));
}

#[test]
fn inputs_that_do_not_load_come_back_unchanged() {
    for option_set in OptionSet::ALL {
        let options = option_set.options();
        for path in suite_files() {
            for case in suite_cases::load_cases_from_file(&path) {
                if loaded_data::data_signature(&case.yaml).is_none() {
                    assert_eq!(
                        format_yaml(&case.yaml, &options),
                        case.yaml,
                        "{option_set:?}: {}[{}]",
                        case.file,
                        case.index
                    );
                }
            }
        }
    }
}

/// A second `:` in one entry of a flow sequence is a syntax error; the input once
/// made `load` run until memory was exhausted.
#[rstest]
#[case::empty_key_and_value("[: :]\n")]
#[case::spaced("[ : : ]\n")]
#[case::explicit_key("[? : :]\n")]
#[case::nested("[[: :]]\n")]
fn a_second_colon_in_a_flow_sequence_entry_does_not_load(#[case] input: &str) {
    assert!(load(input).is_err());
    for option_set in OptionSet::ALL {
        assert_eq!(format_yaml(input, &option_set.options()), input);
    }
}

/// A flow collection or a second `:` after a complete entry, without the `,`
/// before it, is an error; `{ a: 2 !u2 { } }` loaded to data that no output
/// could repeat.
#[rstest]
#[case::mapping_after_a_value("{ a: 2 !u2 { } }\n")]
#[case::sequence_after_a_value("{ a: b [ ] }\n")]
#[case::collection_after_an_item("[ a [ ] ]\n")]
#[case::second_colon_after_an_empty_key("{: :}\n")]
#[case::the_fuzz_input(
    "!u { null: \n#c\n &z-\n2 !u2 { }\n#c\n , #c\n true::\n \n#\n true #c\n  }\n"
)]
fn a_missing_comma_in_a_flow_collection_does_not_load(#[case] input: &str) {
    assert!(load(input).is_err());
    for option_set in OptionSet::ALL {
        assert_eq!(format_yaml(input, &option_set.options()), input);
    }
}

/// A `-` after a tag on the same line is an error; `a: !t - :` once loaded as a
/// mapping with three nodes, and the formatter wrote text that was not there.
#[rstest]
#[case::tag_then_an_entry("!t - b\n")]
#[case::tag_then_an_entry_in_a_value("a: !t - b\n")]
#[case::tag_then_an_empty_entry_in_a_value("a: !t - :\n")]
#[case::tag_then_an_empty_entry_in_an_empty_key(": !t - :\n")]
#[case::tag_then_a_bare_dash_in_a_value("a: !t -\n")]
#[case::tag_then_an_empty_entry_in_an_item("- !t - :\n")]
#[case::tag_then_an_entry_in_an_explicit_key("? !t - b\n")]
#[case::tag_alone_as_a_value_then_a_key_and_an_entry(": !\n?\n! - :")]
fn a_dash_after_a_tag_does_not_load(#[case] input: &str) {
    assert!(load(input).is_err());
    for option_set in OptionSet::ALL {
        assert_eq!(format_yaml(input, &option_set.options()), input);
    }
}

/// A block entry after a tab that follows the indentation is an error; it was
/// read at the wrong column, and the lines under it were lost.
#[rstest]
#[case::sequence_entry("k:\n  \t- a\n")]
#[case::mapping_key("k:\n  \ta: 1\n")]
#[case::explicit_key("k:\n  \t? x\n")]
#[case::under_an_empty_item("- \n  \t- a\n")]
fn a_block_entry_after_tab_indentation_does_not_load(#[case] input: &str) {
    assert!(load(input).is_err());
    for option_set in OptionSet::ALL {
        assert_eq!(format_yaml(input, &option_set.options()), input);
    }
}

/// The report of what the sweeps run over, for the reader of `--nocapture`.
#[test]
fn the_sweeps_run_over_the_suite_the_corpus_and_the_pinned_inputs() {
    let inputs = inputs();
    println!(
        "loaded {}, valid in the suite and not loaded {}, invalid in the suite and loaded {}",
        inputs.loaded.len(),
        inputs.unloadable.len(),
        inputs.invalid_but_loaded,
    );
    let from = |prefix: &str| {
        inputs
            .loaded
            .iter()
            .filter(|input| input.source.starts_with(prefix))
            .count()
    };
    assert!(from("corpus/") >= 5, "the corpus files");
    assert_eq!(from("extra: "), EXTRA_INPUTS.len() + 1);
    assert!(
        inputs.loaded.len() - from("corpus/") - from("extra: ") > 300,
        "the suite cases"
    );
}

// ---- the pinned inputs ------------------------------------------------------

/// Every pinned input admits a variant, so none runs the check on its own text
/// alone.
#[test]
fn every_pinned_input_admits_a_variant() {
    for base in inputs()
        .loaded
        .iter()
        .filter(|input| input.source.starts_with("extra: "))
    {
        let lines: Vec<&str> = base.text.split_inclusive('\n').collect();
        let admitted = [Kind::Trailing, Kind::OwnLine, Kind::OwnLineFlush]
            .into_iter()
            .flat_map(|kind| {
                positions(&lines, kind)
                    .into_iter()
                    .map(move |at| (kind, at))
            })
            .any(|(kind, at)| {
                let (text, added) = with_comments(&base.text, kind, &[at]);
                admit(base, &text, &added).is_ok()
            });
        assert!(admitted, "{}", base.source);
    }
}

/// The output for inputs that once lost comments or data, per option set.
/// `check` shows that the data and the comments stay; these show how.
#[rstest]
#[case::anchor_on_an_empty_document(OptionSet::Default, "&x\n---\ny\n", "&x\n---\ny\n")]
#[case::tag_on_an_empty_document(OptionSet::Default, "!t\n---\ny\n", "!t\n---\ny\n")]
#[case::anchor_on_an_empty_document_with_a_comment(
    OptionSet::Default,
    "&x # c\n---\ny\n",
    "# c\n&x\n---\ny\n"
)]
#[case::tag_on_an_empty_document_before_an_end_marker(
    OptionSet::Default,
    "--- !t\n...\n---\ny\n",
    "---\n!t\n...\n---\ny\n"
)]
#[case::empty_key_and_value_under_a_key(OptionSet::Default, "a: ?\n", "a:\n  :\n")]
#[case::empty_key_and_value_in_an_explicit_entry(
    OptionSet::Default,
    "k:\n  ? \n  : \n",
    "k:\n  :\n"
)]
#[case::empty_key_and_value_with_comments(
    OptionSet::Default,
    "k:\n  ? # a\n  : # b\n",
    "k:\n  # a\n  :  # b\n"
)]
#[case::tag_on_an_empty_item_of_an_indented_sequence(
    OptionSet::IndentlessSequences,
    "a:\n  - !!str\nb: 1\n",
    "a:\n- !!str\nb: 1\n"
)]
#[case::tag_on_an_empty_item_with_comments(
    OptionSet::Default,
    "- !!str # c\n- # d\n- x\n",
    "# c\n- !!str\n# d\n- \n- x\n"
)]
#[case::empty_explicit_entries(OptionSet::Default, "?\n:\n?\n:\n", ":\n:\n")]
#[case::anchor_on_an_empty_explicit_key(OptionSet::Default, "? &x\n: v\n", "&x : v\n")]
#[case::tag_on_an_empty_explicit_key(OptionSet::Default, "? !!str\n? &x\n", "!!str : \n&x : \n")]
#[case::dash_marker_of_a_compact_mapping(
    OptionSet::TabWidthFour,
    "- a: 1\n  b: 2\n",
    "-   a: 1\n    b: 2\n"
)]
#[case::explicit_key_with_a_sequence(
    OptionSet::TabWidthFour,
    "? - a\n  - b\n: c\n",
    "?   - a\n    - b\n: c\n"
)]
#[case::duplicate_key_with_an_alias_keeps_the_anchor(
    OptionSet::RemoveDuplicateKeys,
    "a: &x 1\nb: *x\na: 2\n",
    "a: &x 1\nb: *x\na: 2\n"
)]
#[case::duplicate_merge_keys_stay(
    OptionSet::RemoveDuplicateKeys,
    "<<: *x\n<<: *y\n",
    "<<: *x\n<<: *y\n"
)]
#[case::duplicate_key_with_a_comment(
    OptionSet::RemoveDuplicateKeys,
    "# lead\na: 1 # one\nb: 2\na: 3 # three\n",
    "# lead\n# one\nb: 2\na: 3  # three\n"
)]
#[case::quoted_key_next_to_the_plain_one_in_yaml_1_1(
    OptionSet::Yaml11,
    "y: a\n\"y\": b\n",
    "y: a\n\"y\": b\n"
)]
#[case::quoted_bool_words_in_yaml_1_1(
    OptionSet::Yaml11,
    "a: \"y\"\nb: \"1_000\"\nc: \"0b101\"\nd: \"1:30\"\ne: \"2001-12-14\"\nf: \"x\"\n",
    "a: \"y\"\nb: \"1_000\"\nc: \"0b101\"\nd: \"1:30\"\ne: \"2001-12-14\"\nf: x\n"
)]
#[case::plain_bool_word_in_yaml_1_1_stays_plain(OptionSet::Yaml11, "a: y\n", "a: y\n")]
#[case::quoted_bool_word_in_yaml_1_2_loses_its_quotes(OptionSet::Default, "a: \"y\"\n", "a: y\n")]
#[case::flow_collection_with_a_comment_turns_block(
    OptionSet::Default,
    "{\"foo\" # t0\n: \"bar\"} # t1\n",
    "# t1\nfoo: bar  # t0\n"
)]
#[case::block_scalar_without_content_below_a_comment(
    OptionSet::Default,
    "--- |1-\n# c\n",
    "---\n\"\"\n# c\n"
)]
#[case::tagged_empty_block_scalar_keeps_its_header_comment(
    OptionSet::Default,
    "- !t |  # d\n\n",
    "- !t |  # d\n"
)]
#[case::keep_chomping_of_a_value_without_a_final_line_break(
    OptionSet::Default,
    "a: |+\n  x",
    "a: |-\n  x\n"
)]
#[case::empty_value_with_properties_before_the_closing_brace(
    OptionSet::Default,
    "{ a: !!str \"\", b: !!str }\n",
    "{ a: !!str \"\", b: !!str }\n"
)]
#[case::properties_alone_in_a_flow_sequence(OptionSet::Default, "[ !!str ]\n", "[!!str \"\"]\n")]
#[case::properties_alone_as_a_flow_mapping_key(OptionSet::Default, "{ !!str }\n", "{ !!str :  }\n")]
#[case::anchor_on_a_single_pair_value(OptionSet::Default, "[ a: &x ]\n", "[{ a: &x }]\n")]
#[case::explicit_key_indicator_block_scalar_prints_quoted(
    OptionSet::Default,
    "? >1\n  t\n  u\n: v\n",
    "\" t\\n u\\n\": v\n"
)]
fn formatter_pins(#[case] option_set: OptionSet, #[case] input: &str, #[case] expected: &str) {
    assert!(loaded_data::data_signature(input).is_some(), "{input:?}");
    let options = option_set.options();
    let (output, problem) = check(input, &options);
    assert_eq!(output, expected, "{option_set:?}: {input:?}");
    assert!(problem.is_none(), "{option_set:?}: {input:?}");
}

/// The check compares the output with the loaded input, so a property that the
/// loader drops would drop out of the check as well.  The loader gives a
/// document whose only node is made of properties those properties.
#[rstest]
#[case::anchor_before_a_start_marker("&x\n---\ny\n", Some("x"), None)]
#[case::tag_before_a_start_marker("!t\n---\ny\n", None, Some("!t"))]
#[case::anchor_on_a_marker_line("--- &x\n---\ny\n", Some("x"), None)]
#[case::anchor_before_an_end_marker("&x\n...\n", Some("x"), None)]
#[case::tag_on_a_marker_line_before_an_end_marker("--- !t\n...\n---\ny\n", None, Some("!t"))]
#[case::anchor_with_a_comment("&x # c\n---\ny\n", Some("x"), None)]
#[case::anchor_at_the_end_of_the_input("&x\n", Some("x"), None)]
fn the_loader_keeps_the_properties_of_an_empty_document(
    #[case] input: &str,
    #[case] anchor: Option<&str>,
    #[case] tag: Option<&str>,
) {
    let documents = load(input).unwrap_or_default();
    let root = documents.first().map(|document| &document.root);
    assert_eq!(root.and_then(Node::anchor), anchor, "{input:?}");
    assert_eq!(
        root.and_then(Node::tag_loc).is_some(),
        tag.is_some(),
        "{input:?}"
    );
    if let Some(Node::Scalar { tag: loaded, .. }) = root {
        assert_eq!(
            loaded.as_deref().filter(|t| !t.starts_with("tag:yaml.org")),
            tag,
            "{input:?}"
        );
    }
}

/// An empty scalar with the tag `!!str` and one with the anchor `x`, as
/// `data_signature` writes them.
const EMPTY_STR: &str = "S&None !Some(\"tag:yaml.org,2002:str\")\"\";";
const EMPTY_ANCHORED: &str = "S&Some(\"x\") !Some(\"tag:yaml.org,2002:null\")\"\";";

/// The same for properties that a `]`, `}` or `,` follows in a flow collection:
/// the loaded input carries them (the parser dropped them at the closing
/// delimiter, and the formatter output matched the dropped data).
#[rstest]
#[case::tag_in_a_sequence("[ !!str ]\n", EMPTY_STR)]
#[case::anchor_in_a_sequence("[ &x ]\n", EMPTY_ANCHORED)]
#[case::tag_as_a_mapping_key("{ !!str }\n", EMPTY_STR)]
#[case::tag_as_a_mapping_value("{ b: !!str }\n", EMPTY_STR)]
#[case::anchor_as_a_mapping_value("{ b: &x }\n", EMPTY_ANCHORED)]
#[case::anchor_on_a_single_pair_value("[ a: &x ]\n", EMPTY_ANCHORED)]
#[case::tag_on_a_single_pair_value("[ a: !!str ]\n", EMPTY_STR)]
#[case::tag_on_an_explicit_key("{ ? !!str }\n", EMPTY_STR)]
#[case::anchor_on_the_value_of_a_pair_next_to_an_alias("[{ b: &x }, *x]\n", EMPTY_ANCHORED)]
fn the_loader_keeps_the_properties_of_empty_flow_nodes(
    #[case] input: &str,
    #[case] property: &str,
) {
    let signature = loaded_data::data_signature(input).unwrap_or_default();
    assert!(signature.contains(property), "{input:?}: {signature}");
}

/// The loaded data of a single pair in a flow sequence: properties before the
/// `:` belong to the key, and a `?` forms a pair with an empty key or value
/// (the loader dropped or moved them, and the formatter output matched).
#[rstest]
#[case::core_tag_on_the_key(
    "[ !!str : a ]\n",
    "S&None !Some(\"tag:yaml.org,2002:str\")\"\";S&None !Some(\"tag:yaml.org,2002:str\")\"a\";"
)]
#[case::tag_on_the_key(
    "[ !t : a ]\n",
    "S&None !Some(\"!t\")\"\";S&None !Some(\"tag:yaml.org,2002:str\")\"a\";"
)]
#[case::anchor_on_the_key(
    "[ &x : a ]\n",
    "S&Some(\"x\") !Some(\"tag:yaml.org,2002:null\")\"\";S&None !Some(\"tag:yaml.org,2002:str\")\"a\";"
)]
#[case::anchor_and_tag_on_the_key(
    "[ !t &x : b ]\n",
    "S&Some(\"x\") !Some(\"!t\")\"\";S&None !Some(\"tag:yaml.org,2002:str\")\"b\";"
)]
#[case::tag_on_an_explicit_key(
    "[ ? !t : a ]\n",
    "S&None !Some(\"!t\")\"\";S&None !Some(\"tag:yaml.org,2002:str\")\"a\";"
)]
#[case::tag_on_a_key_without_a_value(
    "[ !t : ]\n",
    "S&None !Some(\"!t\")\"\";S&None !Some(\"tag:yaml.org,2002:null\")\"\";"
)]
#[case::question_mark_alone(
    "[ ? ]\n",
    "S&None !Some(\"tag:yaml.org,2002:null\")\"\";S&None !Some(\"tag:yaml.org,2002:null\")\"\";"
)]
#[case::question_mark_and_key(
    "[ ? a ]\n",
    "S&None !Some(\"tag:yaml.org,2002:str\")\"a\";S&None !Some(\"tag:yaml.org,2002:null\")\"\";"
)]
#[case::question_mark_and_tag(
    "[ ? !!str ]\n",
    "S&None !Some(\"tag:yaml.org,2002:str\")\"\";S&None !Some(\"tag:yaml.org,2002:null\")\"\";"
)]
fn the_loader_keeps_the_pair_of_a_flow_sequence_entry(#[case] input: &str, #[case] pair: &str) {
    let signature = loaded_data::data_signature(input).unwrap_or_default();
    let expected = format!("M&None !Some(\"tag:yaml.org,2002:map\")({pair});");
    assert!(signature.contains(&expected), "{input:?}: {signature}");
}

/// A scalar key that escaping makes longer than an implicit key may be prints in
/// the explicit form, whose key has no limit.
#[test]
fn escaped_key_over_the_implicit_key_limit_prints_explicit() {
    let key = "\u{2028}".repeat(1000);
    for input in [
        format!("\"{key}\": v\n"),
        format!("\"{key}\": v # c\n"),
        format!("# c\n\"{key}\": v\n"),
        format!("a:\n  \"{key}\": v\n"),
    ] {
        let (output, problem) = check(&input, &YamlFormatOptions::default());
        assert!(problem.is_none(), "{}", head(&input));
        assert!(output.contains("? \""), "{}", head(&output));
    }
}

/// The keys at the implicit key limit: counted in characters after escaping.
#[rstest]
#[case::at_the_limit(1022, false)]
#[case::one_over(1023, true)]
fn key_length_decides_between_implicit_and_explicit(#[case] chars: usize, #[case] explicit: bool) {
    // `\L` is two characters, and the quotes two more.
    let key = "\u{2028}".repeat(chars / 2) + &"a".repeat(chars % 2);
    let input = format!("\"{key}\": v\n");
    let (output, problem) = check(&input, &YamlFormatOptions::default());
    assert!(problem.is_none(), "{chars}");
    assert_eq!(output.starts_with("? "), explicit, "{}", head(&output));
}

/// Under a `tab_width` below the width of `- ` the continuation lines of a compact
/// collection would sit left of its first entry.
#[rstest]
#[case::zero(0)]
#[case::one(1)]
#[case::two(2)]
#[case::three(3)]
#[case::eight(8)]
#[case::nine(9)]
#[case::twelve(12)]
fn every_tab_width_keeps_the_data_of_compact_collections(#[case] tab_width: usize) {
    let options = YamlFormatOptions {
        tab_width,
        ..YamlFormatOptions::default()
    };
    for input in [
        "- a: 1\n  b: 2\n",
        "a:\n  - b: 1\n    c: 2\n  - - x\n    - y\n",
        "? - a\n  - b\n: c\n",
        "- ? k\n  : v\n  w: 1 # c\n",
        "a: |2\n   x\nb:\n  - >1\n     y # not a comment\n",
    ] {
        let (_, problem) = check(input, &options);
        assert!(problem.is_none(), "tab_width {tab_width}: {input:?}");
    }
}

// ---- line endings and comment texts -----------------------------------------

/// The pinned inputs and the corpus, written with the line ending of the option.
#[rstest]
#[case::crlf(LineEnding::Crlf, "\r\n")]
#[case::cr(LineEnding::Cr, "\r")]
fn comments_survive_every_line_ending(#[case] line_ending: LineEnding, #[case] terminator: &str) {
    let options = YamlFormatOptions {
        line_ending,
        ..YamlFormatOptions::default()
    };
    let mut checked = 0;
    for base in inputs()
        .loaded
        .iter()
        .filter(|input| input.source.starts_with("extra: ") || input.source.starts_with("corpus/"))
    {
        let text = base.text.replace('\n', terminator);
        if loaded_data::data_signature(&text).is_none() {
            continue;
        }
        let (output, problem) = check(&text, &options);
        assert!(problem.is_none(), "{}: {}", base.source, head(&output));
        checked += 1;
    }
    assert!(checked > 20, "{checked}");
}

/// Comment texts are kept as they are written: leading and trailing white space,
/// tabs, further `#`, quotes, and the line separators U+2028 and U+0085, which
/// the parser takes for text.
#[rstest]
#[case::empty("#")]
#[case::without_a_space("#nospace")]
#[case::tab("#\t x")]
#[case::trailing_spaces("# trailing   ")]
#[case::hash("# # nested")]
#[case::quotes("# \"q\" 'q' \\")]
#[case::non_ascii("# ü日")]
#[case::line_separator("# \u{2028}x")]
#[case::next_line("# \u{85}")]
fn comment_text_is_kept(#[case] comment: &str) {
    for input in [
        format!("a: 1 {comment}\n"),
        format!("{comment}\na: 1\n"),
        format!("a:\n  {comment}\n  b: 1\n"),
        format!("- x {comment}\n- y\n"),
        format!("[a, {comment}\n b]\n"),
    ] {
        for option_set in OptionSet::ALL {
            let (output, problem) = check(&input, &option_set.options());
            assert!(
                problem.is_none(),
                "{option_set:?}: {input:?} gives {output:?}"
            );
        }
    }
}

/// A deeply nested file keeps the comment at its bottom, on a stack that the
/// recursion of the parser, the loader and the formatter fits.
#[test]
fn deeply_nested_input_keeps_its_comment() {
    let handle = std::thread::Builder::new()
        .stack_size(256 * 1024 * 1024)
        .spawn(|| {
            let depth = 200;
            let mut mapping = String::new();
            for level in 0..depth {
                let _ = writeln!(mapping, "{}a:", "  ".repeat(level));
            }
            let _ = writeln!(mapping, "{}b: 1 # bottom", "  ".repeat(depth));
            let mut sequence = String::new();
            for level in 0..depth {
                let _ = writeln!(sequence, "{}-", "  ".repeat(level));
            }
            let _ = writeln!(sequence, "{}- x # bottom", "  ".repeat(depth));
            for input in [mapping, sequence] {
                for option_set in OptionSet::ALL {
                    let (_, problem) = check(&input, &option_set.options());
                    assert!(problem.is_none(), "{option_set:?}");
                }
            }
        });
    assert!(handle.is_ok_and(|handle| handle.join().is_ok()));
}

// ---- comment-positions.yaml -------------------------------------------------

#[test]
fn the_comment_positions_file_holds_each_comment_once() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus/comment-positions.yaml");
    let text = std::fs::read_to_string(path).unwrap_or_default();
    assert!(load(&text).is_ok());
    let comments = comment_texts(&text);
    assert!(comments.len() >= 60, "{}", comments.len());
    let distinct: BTreeSet<&String> = comments.iter().collect();
    assert_eq!(distinct.len(), comments.len(), "a comment text repeats");
}

// ---- the helpers ------------------------------------------------------------

#[rstest]
#[case::leading_space_stays("a: 1 # x\n#y\n", &[" x", "y"])]
#[case::hash_in_a_double_quoted_scalar("a: \"x #y\"\n", &[])]
#[case::hash_in_a_plain_scalar("a: b#c\n", &[])]
#[case::hash_in_a_literal("a: |\n  # not\n", &[])]
#[case::hash_in_a_single_quoted_scalar("a: '# q'\n", &[])]
fn comment_texts_are_the_comment_events(#[case] text: &str, #[case] expected: &[&str]) {
    assert_eq!(comment_texts(text), expected);
}

#[test]
fn multiset_difference_counts_repeats() {
    let owned = |texts: &[&str]| texts.iter().map(ToString::to_string).collect::<Vec<_>>();
    assert_eq!(
        multiset_difference(&owned(&[" a", " a"]), &owned(&[" a"])),
        owned(&[" a"])
    );
    assert!(multiset_difference(&owned(&[" a"]), &owned(&[" a", " a"])).is_empty());
    assert_eq!(
        multiset_difference(&owned(&[" a", " b"]), &owned(&[" b"])),
        owned(&[" a"])
    );
}

#[rstest]
#[case::lf("a: 1\nb: 2\n", &[0], "a: 1 # t0\nb: 2\n")]
#[case::crlf("a: 1\r\nb: 2\r\n", &[0], "a: 1 # t0\r\nb: 2\r\n")]
#[case::last_line_without_a_terminator("a: 1", &[0], "a: 1 # t0")]
#[case::two_lines("a: 1\nb: 2\n", &[0, 1], "a: 1 # t0\nb: 2 # t1\n")]
#[case::line_separator_is_no_line_break("a: \u{2028}x\nb: 2\n", &[0], "a: \u{2028}x # t0\nb: 2\n")]
fn trailing_comment_goes_before_the_line_terminator(
    #[case] text: &str,
    #[case] at: &[usize],
    #[case] expected: &str,
) {
    let (variant, added) = with_comments(text, Kind::Trailing, at);
    assert_eq!(variant, expected);
    assert_eq!(added.len(), at.len());
}

#[rstest]
#[case::indented("a:\n  b: 1\n", Kind::OwnLine, &[1], "a:\n  # o1\n  b: 1\n")]
#[case::flush("a:\n  b: 1\n", Kind::OwnLineFlush, &[1], "a:\n# f1\n  b: 1\n")]
#[case::before_the_first_line("a: 1\n", Kind::OwnLine, &[0], "# o0\na: 1\n")]
#[case::after_the_last_line("a: 1\n", Kind::OwnLine, &[1], "a: 1\n# o1\n")]
#[case::after_a_last_line_without_a_terminator("a: 1", Kind::OwnLine, &[1], "a: 1\n# o1\n")]
fn own_line_comment_goes_before_the_line(
    #[case] text: &str,
    #[case] kind: Kind,
    #[case] at: &[usize],
    #[case] expected: &str,
) {
    assert_eq!(with_comments(text, kind, at).0, expected);
}

#[test]
fn every_line_of_a_long_input_is_a_position() {
    let lines: Vec<String> = (0..500).map(|i| format!("k{i}: v\n")).collect();
    let lines: Vec<&str> = lines.iter().map(String::as_str).collect();
    assert_eq!(
        positions(&lines, Kind::Trailing),
        (0..500).collect::<Vec<_>>()
    );
    assert_eq!(
        positions(&lines, Kind::OwnLine),
        (0..=500).collect::<Vec<_>>()
    );
}

#[test]
fn trailing_comments_skip_blank_and_comment_lines() {
    let lines = ["a: 1\n", "\n", "# c\n", "  \n", "b: 2\n"];
    assert_eq!(positions(&lines, Kind::Trailing), vec![0, 4]);
}

#[rstest]
#[case::last_wins("a: 1\na: 2\n", "a: 2\n")]
#[case::quoted_and_plain_are_one_key("a: 1\n\"a\": 2\n", "\"a\": 2\n")]
#[case::int_and_string_are_two_keys("1: a\n\"1\": b\n", "1: a\n\"1\": b\n")]
#[case::anchor_keeps_the_entry("a: &x 1\nb: *x\na: 2\n", "a: &x 1\nb: *x\na: 2\n")]
#[case::merge_keys_stay("<<: *x\n<<: *y\n", "<<: *x\n<<: *y\n")]
#[case::nested("k:\n  a: 1\n  a: 2\n", "k:\n  a: 2\n")]
fn the_reference_removal_of_duplicate_keys(#[case] text: &str, #[case] expected: &str) {
    let options = OptionSet::RemoveDuplicateKeys.options();
    assert_eq!(
        expected_data(text, &options),
        loaded_data::data_signature(expected).unwrap_or_default(),
    );
}

#[test]
fn the_reference_tells_quoted_and_plain_keys_apart_in_yaml_1_1_only() {
    let text = "y: a\n\"y\": b\n";
    let kept = loaded_data::data_signature(text).unwrap_or_default();
    assert_eq!(expected_data(text, &OptionSet::Yaml11.options()), kept);
    let deduped = YamlFormatOptions {
        format_remove_duplicate_keys: true,
        yaml_version: YamlVersion::V1_1,
        ..YamlFormatOptions::default()
    };
    assert_eq!(expected_data(text, &deduped), kept);
    let one_key = YamlFormatOptions {
        yaml_version: YamlVersion::V1_2,
        ..deduped
    };
    assert_ne!(expected_data(text, &one_key), kept);
}
