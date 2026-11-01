// SPDX-License-Identifier: MIT
//
// Formatter round-trip conformance test using yaml-test-suite data at
// `tests/yaml-test-suite/src/` (workspace root, relative path `../tests/`).
//
// For each non-fail case in the suite:
//   1. `format_yaml(input, &opts)` at print widths 80 and 20 must produce output that parses
//      cleanly (no diagnostics).
//   2. Formatting must be idempotent: `format(format(input)) == format(input)`.
//   3. Every comment around a document's root (before `---`, after `---`, after
//      the root, after `...`) in the input must be in the output.  The output may
//      hold more: a comment on an indicator line (`- # c`) moves to its own line
//      above the node, where the parser reads it as a document-level comment
//      when the node is the first of the document.
//   4. The output holds no comment event the input lacks, so a comment is neither
//      invented nor printed twice.  Every comment at a node position is in
//      `formatter_keeps_comments.rs`.
//   5. The output of a case that loads loads to the same data (see
//      `formatter_keeps_the_data_of_suite_case`).
//
// The same comment checks run over the real files in `tests/corpus/`.

#![expect(missing_docs, reason = "test code")]

mod loaded_data;
mod suite_cases;

use std::path::PathBuf;
use std::time::Duration;

use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::parser::parse_yaml;
use rlsp_yaml_parser::Event;
use rstest::rstest;
use suite_cases::load_cases_from_file;

// ---- Comment comparison -------------------------------------------------------

/// The comments around each document's root, per document: before `---`, after
/// `---`, after the root, after `...`.  A stream that loads as no document
/// yields its comment events as one entry.  `None` when the text does not load.
fn document_comment_lists(text: &str) -> Option<Vec<[Vec<String>; 4]>> {
    let docs = rlsp_yaml_parser::load(text).ok()?;
    if docs.is_empty() {
        let comments = rlsp_yaml_parser::parse_events(text)
            .filter_map(|event| match event {
                Ok((Event::Comment { text }, _)) => Some(format!("#{text}")),
                _ => None,
            })
            .collect();
        return Some(vec![[comments, Vec::new(), Vec::new(), Vec::new()]]);
    }
    Some(
        docs.into_iter()
            .map(|d| {
                [
                    d.prefix_comments,
                    d.comments,
                    d.end_comments,
                    d.after_end_comments,
                ]
            })
            .collect(),
    )
}

/// The first comment around a document's root in `input` that `output` lacks,
/// counting repeats.  `None` when `output` has all of them or `input` does not load.
fn missing_document_comment(input: &str, output: &str) -> Option<String> {
    let flatten = |lists: Vec<[Vec<String>; 4]>| -> Vec<String> {
        lists.into_iter().flatten().flatten().collect()
    };
    let mut available = flatten(document_comment_lists(output).unwrap_or_default());
    let mut missing = None;
    for comment in flatten(document_comment_lists(input)?) {
        if let Some(pos) = available.iter().position(|c| *c == comment) {
            available.swap_remove(pos);
        } else {
            missing = Some(comment);
            break;
        }
    }
    missing
}

/// The first comment event of `output` that `input` does not hold as often,
/// counting repeats.  `None` when every one is accounted for or `input` does
/// not parse.
fn invented_comment(input: &str, output: &str) -> Option<String> {
    let comment_events = |text: &str| -> Option<Vec<String>> {
        rlsp_yaml_parser::parse_events(text)
            .filter_map(|event| match event {
                Ok((Event::Comment { text }, _)) => Some(Ok(format!("#{text}"))),
                Ok(_) => None,
                Err(e) => Some(Err(e)),
            })
            .collect::<Result<Vec<_>, _>>()
            .ok()
    };
    let mut available = comment_events(input)?;
    for comment in comment_events(output).unwrap_or_default() {
        if let Some(pos) = available.iter().position(|c| *c == comment) {
            available.swap_remove(pos);
        } else {
            return Some(comment);
        }
    }
    None
}

// ---- Panic helper -----------------------------------------------------------

/// Terminate the test with a clear message.
#[expect(clippy::panic, reason = "test harness reports failures via panic")]
fn fail(msg: &str) -> ! {
    panic!("{msg}")
}

// ---- rstest harness ---------------------------------------------------------

#[rstest]
#[timeout(Duration::from_secs(5))]
fn formatter_conformance(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
    #[values(80, 20)] print_width: usize,
) {
    let cases = load_cases_from_file(&path);
    if cases.is_empty() {
        // All entries are skipped (e.g., ZYU8). Nothing to test.
        return;
    }

    let opts = YamlFormatOptions {
        print_width,
        ..YamlFormatOptions::default()
    };

    for case in &cases {
        let tag = format!("{}[{}] {}", case.file, case.index, case.name);

        // Intentionally-invalid YAML is not formatted.
        if case.fail {
            continue;
        }

        let input = &case.yaml;
        let output = format_yaml(input, &opts);

        // Parse-clean assertion.
        // Every comment survives formatting, so a comment-only stream is not
        // empty; empty output is valid only for a stream without a comment.
        let diagnostics = parse_yaml(&output).diagnostics;
        let parse_failed = !diagnostics.is_empty();

        // Idempotency assertion.
        let second = format_yaml(&output, &opts);
        let idempotent_failed = output != second;

        if parse_failed {
            let diag_msgs: Vec<String> = diagnostics.iter().map(|d| d.message.clone()).collect();
            fail(&format!(
                "formatter_conformance: {tag} — formatted output does not parse cleanly\n  \
                 input:       {:?}\n  \
                 output:      {:?}\n  \
                 diagnostics: {:?}",
                &input[..input.len().min(120)],
                &output[..output.len().min(120)],
                diag_msgs,
            ));
        }

        assert_eq!(
            missing_document_comment(input, &output),
            None,
            "formatter_conformance: {tag} — document-level comment lost\n  \
             input:  {:?}\n  \
             output: {:?}",
            &input[..input.len().min(120)],
            &output[..output.len().min(120)],
        );

        assert_eq!(
            invented_comment(input, &output),
            None,
            "formatter_conformance: {tag} — comment invented or printed twice\n  \
             input:  {:?}\n  \
             output: {:?}",
            &input[..input.len().min(120)],
            &output[..output.len().min(120)],
        );

        assert!(
            !idempotent_failed,
            "formatter_conformance: {tag} — formatter is not idempotent\n  \
             input:  {:?}\n  \
             first:  {:?}\n  \
             second: {:?}",
            &input[..input.len().min(120)],
            &output[..output.len().min(200)],
            &second[..second.len().min(200)],
        );
    }
}

/// The output of every suite case that loads loads to the same data, whatever
/// the options say about style: structure, values, tags, anchors and aliases.
#[rstest]
#[timeout(Duration::from_secs(5))]
fn formatter_keeps_the_data_of_suite_case(
    #[files("../tests/yaml-test-suite/src/*.yaml")] path: PathBuf,
) {
    let base = YamlFormatOptions::default;
    let option_sets = [
        base(),
        YamlFormatOptions {
            format_enforce_block_style: true,
            ..base()
        },
        YamlFormatOptions {
            print_width: 20,
            ..base()
        },
        YamlFormatOptions {
            single_quote: true,
            ..base()
        },
        YamlFormatOptions {
            format_indent_sequences: false,
            ..base()
        },
        YamlFormatOptions {
            preserve_quotes: true,
            ..base()
        },
    ];
    for case in load_cases_from_file(&path).iter().filter(|case| !case.fail) {
        let Some(expected) = loaded_data::data_signature(&case.yaml) else {
            continue;
        };
        for options in &option_sets {
            let output = format_yaml(&case.yaml, options);
            assert_eq!(
                loaded_data::data_signature(&output).as_deref(),
                Some(expected.as_str()),
                "{}[{}] {}: the output holds other data\n  input:  {:?}\n  output: {:?}",
                case.file,
                case.index,
                case.name,
                case.yaml,
                output,
            );
        }
    }
}

/// The real files in `tests/corpus/` keep the comments around their documents.
#[rstest]
fn formatter_keeps_document_comments_of_corpus_file(#[files("tests/corpus/*.y*ml")] path: PathBuf) {
    let input = std::fs::read_to_string(&path).unwrap();
    let output = format_yaml(&input, &YamlFormatOptions::default());
    assert_eq!(
        missing_document_comment(&input, &output),
        None,
        "{}: document-level comment lost",
        path.display()
    );
    assert_eq!(
        invented_comment(&input, &output),
        None,
        "{}: comment invented or printed twice",
        path.display()
    );
    assert_eq!(
        format_yaml(&output, &YamlFormatOptions::default()),
        output,
        "{}: formatter is not idempotent",
        path.display()
    );
}
