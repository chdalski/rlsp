// SPDX-License-Identifier: MIT

use std::fmt::Write as _;

use rlsp_fmt::{Doc, FormatOptions, format as fmt_format, hard_line, join, text};
use rlsp_yaml_parser::node::{Document, Node};
use rlsp_yaml_parser::{Event, ScalarStyle, Span};

use crate::editing::editor_config::LineEnding;

mod blank_lines;
mod dedup;
mod flow_comments;
mod mapping_render;
mod node_to_doc;
/// YAML formatting options and their defaults.
pub mod options;
mod scalar_render;
mod sequence_render;

pub(crate) use node_to_doc::prints_block_header;
pub use options::YamlFormatOptions;

/// Format a single AST node to text.
///
/// Renders `node` using the same `rlsp-fmt` path as `format_yaml`.
/// The first output line starts at column 0; the caller is responsible for
/// positioning it within the larger document.  Every continuation line
/// (lines 2 and beyond) receives `base_indent` leading spaces so the output
/// aligns with the surrounding structure.
///
/// Empty collections (`{}`, `[]`) are emitted inline regardless of their
/// `CollectionStyle`, matching `node_to_doc`'s short-circuit behavior.
#[must_use]
pub fn format_subtree(
    node: &Node<Span>,
    options: &YamlFormatOptions,
    base_indent: usize,
) -> String {
    let doc = node_to_doc::node_to_doc(node, options, false);
    let fmt_options = FormatOptions {
        print_width: options.print_width,
        tab_width: options.indent_width(),
        use_tabs: false,
    };
    let rendered = fmt_format(&doc, &fmt_options);
    // Strip the trailing newline that fmt_format appends, then re-join lines
    // with base_indent prepended to every continuation line.
    let text = rendered.trim_end_matches('\n');
    if base_indent == 0 {
        return text.to_string();
    }
    let indent_str = " ".repeat(base_indent);
    let mut lines = text.lines();
    lines.next().map_or_else(String::new, |first| {
        let rest = lines.fold(String::new(), |mut acc, l| {
            let _ = write!(acc, "\n{indent_str}{l}");
            acc
        });
        format!("{first}{rest}")
    })
}

/// Format a YAML document string.
///
/// Returns the formatted text. If the input fails to parse, returns the
/// original text unchanged so the caller never loses content.
///
/// Every comment reaches the output from the AST: the loader stores the
/// comments around a document's root in the document (before `---`, after
/// `---`, after the root, after the last `...`) and every other comment in the
/// node it belongs to.  A stream that holds only comments loads as no
/// document, so its comments come from the parser's event stream.  A comment
/// on a `---`, `...`, or directive line moves to its own line: below the
/// `---` or `...` marker, above the `---` that follows the directive.  The
/// input is consulted only to restore its blank lines.
#[must_use]
pub fn format_yaml(text_input: &str, options: &YamlFormatOptions) -> String {
    // The parser preserves scalar styles (plain, quoted, block) and tags natively.
    // No special configuration needed — every scalar carries its original style.
    let documents: Vec<Document<Span>> = match rlsp_yaml_parser::load(text_input) {
        Ok(docs) => docs,
        Err(_) => return text_input.to_string(),
    };

    let source_blocks = blank_lines::block_scalar_lines(&documents);
    let documents = settle_documents(documents, options);

    let mut result = if documents.is_empty() {
        match stream_comments(text_input) {
            Some(comments) if comments.is_empty() => return String::new(),
            Some(comments) => comments.join("\n"),
            None => return text_input.to_string(),
        }
    } else {
        render_documents(&documents, options)
    };

    // The rendered lines are joined, not terminated: end the last one.  A block
    // scalar that ends in blank lines needs this too, so the newline is not
    // conditional on how the output ends.
    result.push('\n');

    // Blank lines are not in the AST; copy them from the input.
    // restore_blank_lines always produces LF output with a trailing newline.
    result = blank_lines::restore_blank_lines(text_input, &source_blocks, &result);

    // Apply line-ending substitution: replace all LF with the requested terminator.
    // restore_blank_lines produces only LF, so this replace is safe (no CR complications).
    match options.line_ending {
        LineEnding::Lf => {}
        LineEnding::Crlf => {
            result = result.replace('\n', "\r\n");
        }
        LineEnding::Cr => {
            result = result.replace('\n', "\r");
        }
    }

    // Apply insert_final_newline policy: strip exactly one trailing terminator,
    // unless the value of the last block scalar ends in it: `a: |+\n  x\n` loses
    // its line break, and with it its value, when nothing follows.
    if !options.insert_final_newline {
        let stripped = strip_final_terminator(&result, options.line_ending);
        if blank_lines::same_content(stripped, &documents) {
            result.truncate(stripped.len());
        }
    }

    result
}

/// `output` without its last line terminator, if it ends in one.
fn strip_final_terminator(output: &str, line_ending: LineEnding) -> &str {
    let terminator = match line_ending {
        LineEnding::Lf => "\n",
        LineEnding::Crlf => "\r\n",
        LineEnding::Cr => "\r",
    };
    output.strip_suffix(terminator).unwrap_or(output)
}

/// The comments of a stream that holds no document, in source order, each with
/// its `#`.  `None` when the stream does not parse.
fn stream_comments(text_input: &str) -> Option<Vec<String>> {
    rlsp_yaml_parser::parse_events(text_input)
        .filter_map(|event| match event {
            Ok((Event::Comment { text }, _)) => Some(Ok(format!("#{text}"))),
            Ok(_) => None,
            Err(e) => Some(Err(e)),
        })
        .collect::<Result<Vec<_>, _>>()
        .ok()
}

/// The pre-passes over the AST: duplicate-key removal when enabled, then the
/// tags of the collections that the renderers would drop, then the style of the
/// collections that cannot print as written (after the removal, which can move a
/// comment into a flow collection, and after the tags, since a collection with a
/// printed tag takes an explicit key where one without does not).
fn settle_documents(
    documents: Vec<Document<Span>>,
    options: &YamlFormatOptions,
) -> Vec<Document<Span>> {
    documents
        .into_iter()
        .map(|mut doc| {
            if options.format_remove_duplicate_keys {
                dedup::dedup_mapping_keys(&mut doc.root, options.yaml_version);
            }
            mapping_render::spell_collection_tags(&mut doc.root);
            flow_comments::settle_collection_styles(
                &mut doc.root,
                options.format_enforce_block_style,
            );
            quote_block_scalars_with_indent_indicator(
                &mut doc.root,
                false,
                options.indent_width() <= MAX_INDICATOR_INDENT,
            );
            if !doc.end_comments.is_empty() {
                quote_block_scalar_without_content(&mut doc.root);
            }
            doc
        })
        .collect()
}

/// Print a block scalar that needs an indentation indicator double-quoted where
/// the indicator cannot say how far its content is indented: a key is printed
/// one level in from its `?`, so its content sits a level deeper than the
/// indicator counts, and the indicator is one digit, which an indent width above
/// 9 does not fit.
fn quote_block_scalars_with_indent_indicator(
    node: &mut Node<Span>,
    is_key: bool,
    digit_fits: bool,
) {
    match node {
        Node::Scalar { value, style, .. } => {
            if matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_))
                && (is_key || !digit_fits)
                && scalar_render::block_value_needs_indent_indicator(value)
            {
                *style = ScalarStyle::DoubleQuoted;
            }
        }
        Node::Mapping { entries, .. } => {
            for (key, value) in entries.iter_mut() {
                quote_block_scalars_with_indent_indicator(key, true, digit_fits);
                quote_block_scalars_with_indent_indicator(value, false, digit_fits);
            }
        }
        Node::Sequence { items, .. } => {
            for item in items {
                quote_block_scalars_with_indent_indicator(item, false, digit_fits);
            }
        }
        Node::Alias { .. } => {}
    }
}

/// The widest indent whose width is one digit, the most that the indentation
/// indicator of a block scalar holds.
const MAX_INDICATOR_INDENT: usize = 9;

/// Print a block scalar that has no line of content double-quoted.  At the root
/// its content may start in column 0, so the first comment line below it would be
/// content and not a comment.  `--- |1-` below which `# c` stands is the document
/// `""` and a comment; as `|-` it would be the document `# c`.
fn quote_block_scalar_without_content(root: &mut Node<Span>) {
    if let Node::Scalar { value, style, .. } = root
        && matches!(style, ScalarStyle::Literal(_) | ScalarStyle::Folded(_))
        && value.chars().all(|c| c == '\n')
    {
        *style = ScalarStyle::DoubleQuoted;
    }
}

/// Render loaded documents with `rlsp-fmt`: for each document its comments, the
/// `---` and `...` markers its position or `explicit_end` calls for, and the root.
fn render_documents(documents: &[Document<Span>], options: &YamlFormatOptions) -> String {
    let fmt_options = FormatOptions {
        print_width: options.print_width,
        tab_width: options.indent_width(),
        use_tabs: false,
    };

    // One entry per output line, except that a root renders as one entry.
    //
    // Rules:
    // - Emit `---` before a document when it has `explicit_start: true` (first
    //   doc) or when it is not the first document (separator always required).
    // - Emit `...` after a document when it has `explicit_end: true`.
    let comment_lines =
        |comments: &[String]| -> Vec<Doc> { comments.iter().map(|c| text(c.clone())).collect() };
    let mut lines: Vec<Doc> = Vec::new();
    for (i, doc) in documents.iter().enumerate() {
        lines.extend(comment_lines(&doc.prefix_comments));
        // A document with nothing to print needs its `---`: without it, no
        // document is there (`&x` alone loads as one, and so does the one before
        // a `---`).
        if i > 0 || doc.explicit_start || node_to_doc::renders_empty(&doc.root) {
            lines.push(text("---"));
        }
        lines.extend(comment_lines(&doc.comments));
        let ends_output = i + 1 == documents.len()
            && doc.end_comments.is_empty()
            && !doc.explicit_end
            && doc.after_end_comments.is_empty();
        // An empty root that ends the output is no line at all; the final
        // newline ends the line before it.
        if !(ends_output && prints_nothing(&doc.root)) {
            // The comment behind a flow collection that turned block is printed
            // before the comments inside it, which take the end of the lines.
            let root_comment = mapping_render::line_end_comment(&doc.root);
            let (above, line_end) = if mapping_render::is_block_collection(
                &doc.root,
                options.format_enforce_block_style,
            ) {
                (root_comment, None)
            } else {
                (None, root_comment)
            };
            lines.extend(above.map(|comment| text(comment.to_string())));
            lines.push(mapping_render::with_line_end_comment(
                node_to_doc::node_to_doc(&doc.root, options, false),
                line_end,
            ));
        }
        lines.extend(comment_lines(&doc.end_comments));
        if doc.explicit_end {
            lines.push(text("..."));
        }
        lines.extend(comment_lines(&doc.after_end_comments));
    }

    strip_blank_line_indent(&fmt_format(&join(&hard_line(), lines), &fmt_options))
}

/// Whether the formatter prints nothing for `node`: an empty plain scalar with
/// no anchor, no tag that is printed, and no trailing comment that is printed.
fn prints_nothing(node: &Node<Span>) -> bool {
    node_to_doc::renders_empty(node) && mapping_render::line_end_comment(node).is_none()
}

/// Empty the lines that hold only spaces.  The printer indents the line after
/// every line break, so a blank line (of a block scalar's value, say) inside an
/// indented collection comes out as trailing spaces.  No other line of the
/// output consists of spaces alone: a block scalar line of spaces alone is
/// printed double-quoted instead.
fn strip_blank_line_indent(rendered: &str) -> String {
    rendered
        .split('\n')
        .map(|l| if l.bytes().all(|b| b == b' ') { "" } else { l })
        .collect::<Vec<_>>()
        .join("\n")
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};
    use rlsp_yaml_parser::CollectionStyle;
    use rstest::rstest;

    use super::*;
    use crate::server::YamlVersion;

    fn default_opts() -> YamlFormatOptions {
        YamlFormatOptions::default()
    }

    // ---- Group: Multi-contains checks (basic formatting) ----

    #[rstest]
    #[case::boolean_values("enabled: true\ndisabled: false\n", &["true", "false"] as &[&str])]
    #[case::numeric_values("port: 8080\nratio: 0.5\n", &["8080", "0.5"])]
    #[case::mapping_block_style("a: 1\nb: 2\n", &["a: 1", "b: 2"])]
    #[case::flow_sequence_items("items:\n  - a\n  - b\n  - c\n", &["a", "b", "c"])]
    #[case::multi_document(
        "key1: value1\n---\nkey2: value2\n",
        &["key1: value1", "---", "key2: value2"]
    )]
    #[case::float_special_values(
        "nan_val: .nan\ninf_val: .inf\nneg_inf_val: -.inf\n",
        &[".nan", ".inf", "-.inf"]
    )]
    #[case::tagged_node("tagged: !mytag some_value\n", &["!mytag", "some_value"])]
    #[case::literal_block_scalar(
        "body: |\n  line one\n  line two\n",
        &["|", "line one", "line two"]
    )]
    #[case::folded_block_scalar("body: >\n  folded line\n", &[">", "folded line"])]
    #[case::single_quoted_scalar_content("key: 'quoted value'\n", &["quoted value", "key:"])]
    #[case::double_quoted_scalar_content("key: \"quoted value\"\n", &["quoted value", "key:"])]
    fn format_yaml_multi_contains(#[case] input: &str, #[case] expected: &[&str]) {
        let result = format_yaml(input, &default_opts());
        for &s in expected {
            assert!(result.contains(s), "{s:?} missing: {result:?}");
        }
    }

    // ---- Group: Flow collections stay valid, stable, and data-preserving ----

    // A flow collection that exceeds `print_width` breaks one item per line; its
    // closing bracket sits at the items' column so the output loads again.
    #[rstest]
    #[case::flow_seq_under_top_key(
        "items: [alpha, bravo, charlie]",
        20,
        true,
        2,
        "items: [\n  alpha,\n  bravo,\n  charlie\n  ]\n"
    )]
    #[case::flow_seq_under_nested_key(
        "k:\n  items: [alpha, bravo, charlie]",
        20,
        true,
        2,
        "k:\n  items: [\n    alpha,\n    bravo,\n    charlie\n    ]\n"
    )]
    #[case::flow_map_under_key(
        "meta: {alpha: one, bravo: two, charlie: three}",
        20,
        true,
        2,
        "meta: {\n  alpha: one,\n  bravo: two,\n  charlie: three\n  }\n"
    )]
    #[case::flow_map_under_nested_key(
        "k:\n  meta: {alpha: one, bravo: two, charlie: three}",
        20,
        true,
        2,
        "k:\n  meta: {\n    alpha: one,\n    bravo: two,\n    charlie: three\n    }\n"
    )]
    #[case::flow_map_bracket_spacing_false(
        "meta: {alpha: one, bravo: two, charlie: three}",
        20,
        false,
        2,
        "meta: {\n  alpha: one,\n  bravo: two,\n  charlie: three\n  }\n"
    )]
    #[case::flow_seq_in_seq_item(
        "- [alpha, bravo, charlie]",
        20,
        true,
        2,
        "- [\n  alpha,\n  bravo,\n  charlie\n  ]\n"
    )]
    #[case::flow_map_in_seq_item(
        "- {alpha: one, bravo: two, charlie: three}",
        20,
        true,
        2,
        "- {\n  alpha: one,\n  bravo: two,\n  charlie: three\n  }\n"
    )]
    #[case::flow_seq_as_mapping_value_in_seq_item(
        "- key: [alpha, bravo, charlie]",
        20,
        true,
        2,
        "- key: [\n    alpha,\n    bravo,\n    charlie\n    ]\n"
    )]
    #[case::seq_of_seqs_item(
        "- - [alpha, bravo, charlie]",
        20,
        true,
        2,
        "- \n  - [\n    alpha,\n    bravo,\n    charlie\n    ]\n"
    )]
    #[case::nested_flow_in_flow_seq(
        "k: [[alpha, bravo], [charlie, delta]]",
        20,
        true,
        2,
        "k: [\n  [alpha, bravo],\n  [charlie, delta]\n  ]\n"
    )]
    #[case::flow_map_in_flow_seq(
        "k: [{a: 1, b: 2}, {c: 3, d: 4}]",
        12,
        true,
        2,
        "k: [\n  {\n    a: 1,\n    b: 2\n    },\n  {\n    c: 3,\n    d: 4\n    }\n  ]\n"
    )]
    #[case::flow_seq_in_flow_map(
        "k: {x: [alpha, bravo], y: [charlie, delta]}",
        16,
        true,
        2,
        "k: {\n  x: [\n    alpha,\n    bravo\n    ],\n  y: [\n    charlie,\n    delta\n    ]\n  }\n"
    )]
    #[case::single_overlong_item(
        "k: [averyveryverylongscalarvalue]",
        10,
        true,
        2,
        "k: [\n  averyveryverylongscalarvalue\n  ]\n"
    )]
    #[case::anchor_and_tag_before_bracket(
        "k: &a !t [alpha, bravo, charlie]",
        24,
        true,
        2,
        "k: &a !t [\n  alpha,\n  bravo,\n  charlie\n  ]\n"
    )]
    #[case::explicit_key_flow_seq(
        "? [alpha, bravo, charlie]\n: v",
        20,
        true,
        2,
        "? [\n    alpha,\n    bravo,\n    charlie\n    ]\n: v\n"
    )]
    #[case::flow_seq_as_value_of_explicit_key(
        "? k\n: [alpha, bravo, charlie]",
        20,
        true,
        2,
        "k: [\n  alpha,\n  bravo,\n  charlie\n  ]\n"
    )]
    #[case::tab_width_4(
        "items: [alpha, bravo, charlie]",
        20,
        true,
        4,
        "items: [\n    alpha,\n    bravo,\n    charlie\n    ]\n"
    )]
    #[case::top_level_flow_seq(
        "[alpha, bravo, charlie]",
        10,
        true,
        2,
        "[\n  alpha,\n  bravo,\n  charlie\n  ]\n"
    )]
    #[case::top_level_flow_map(
        "{alpha: one, bravo: two}",
        10,
        true,
        2,
        "{\n  alpha: one,\n  bravo: two\n  }\n"
    )]
    #[case::flow_seq_in_deeply_nested_mapping(
        "a:\n  b:\n    c: [alpha, bravo, charlie]",
        20,
        true,
        2,
        "a:\n  b:\n    c: [\n      alpha,\n      bravo,\n      charlie\n      ]\n"
    )]
    #[case::quoted_item_with_comma_keeps_quotes(
        "- [ a, \"Up, up and away!\", b ]\n",
        80,
        true,
        2,
        "- [a, \"Up, up and away!\", b]\n"
    )]
    #[case::quoted_item_with_comma_keeps_quotes_when_broken(
        "- [ a, \"Up, up and away!\", b ]\n",
        20,
        true,
        2,
        "- [\n  a,\n  \"Up, up and away!\",\n  b\n  ]\n"
    )]
    #[case::quoted_mapping_value_and_key_keep_quotes(
        "a: {b: \"x, y\", \"c]d\": 1}\n",
        80,
        true,
        2,
        "a: { b: \"x, y\", \"c]d\": 1 }\n"
    )]
    #[case::single_quoted_item_with_comma("k: [ 'x, y' ]\n", 80, true, 2, "k: [\"x, y\"]\n")]
    #[case::anchored_quoted_item_with_comma(
        "k: [ &a \"x, y\" ]\n",
        80,
        true,
        2,
        "k: [&a \"x, y\"]\n"
    )]
    #[case::tagged_quoted_item_with_comma(
        "k: [ !t \"x, y\" ]\n",
        80,
        true,
        2,
        "k: [!t \"x, y\"]\n"
    )]
    #[case::quoted_trailing_colon_item_in_flow_seq(
        "k: [\"a:\", b]\n",
        80,
        true,
        2,
        "k: [\"a:\", b]\n"
    )]
    #[case::quoted_trailing_colon_value_in_flow_map(
        "k: {x: \"a:\"}\n",
        80,
        true,
        2,
        "k: { x: \"a:\" }\n"
    )]
    #[case::quoted_trailing_colon_block_value("k: \"a:\"\n", 80, true, 2, "k: \"a:\"\n")]
    #[case::quoted_trailing_colon_block_seq_item("- \"a:\"\n", 80, true, 2, "- \"a:\"\n")]
    #[case::quoted_trailing_colon_block_key("\"a:\": 1\n", 80, true, 2, "\"a:\": 1\n")]
    #[case::quoted_trailing_colon_flow_key("{\"a:\": 1}\n", 80, true, 2, "{ \"a:\": 1 }\n")]
    fn flow_output_is_valid_and_stable(
        #[case] input: &str,
        #[case] print_width: usize,
        #[case] bracket_spacing: bool,
        #[case] tab_width: usize,
        #[case] expected: &str,
    ) {
        let opts = YamlFormatOptions {
            print_width,
            tab_width,
            bracket_spacing,
            ..default_opts()
        };
        let output = format_yaml(input, &opts);
        assert_eq!(output, expected);
        assert!(
            rlsp_yaml_parser::load(&output).is_ok(),
            "output does not load: {output:?}"
        );
        assert_eq!(format_yaml(&output, &opts), output, "second format differs");

        // Same data: both sides normalise to block style, which has no layout choices.
        let block_opts = YamlFormatOptions {
            format_enforce_block_style: true,
            ..opts
        };
        assert_eq!(
            format_yaml(input, &block_opts),
            format_yaml(&output, &block_opts),
            "output carries different data than the input"
        );
    }

    // `single_quote` decides the quote style of a flow item that needs quotes,
    // except for keys and for values holding `'`, which stay double-quoted.
    #[rstest]
    #[case::single_quoted_source("k: ['x, y']\n", "k: ['x, y']\n")]
    #[case::double_quoted_source("k: [\"x, y\"]\n", "k: ['x, y']\n")]
    #[case::value_with_apostrophe("k: [\"it's, ok\"]\n", "k: [\"it's, ok\"]\n")]
    #[case::flow_key("{\"x,y\": 1}\n", "{ \"x,y\": 1 }\n")]
    fn flow_item_with_indicator_honours_single_quote(#[case] input: &str, #[case] expected: &str) {
        let opts = YamlFormatOptions {
            single_quote: true,
            ..default_opts()
        };
        let output = format_yaml(input, &opts);
        assert_eq!(output, expected);
        assert_eq!(format_yaml(&output, &opts), output, "second format differs");
    }

    // ---- Group K: Anchor preservation ----

    // K1: Anchor on a scalar value is preserved.
    #[test]
    fn anchor_scalar_preserved() {
        let result = format_yaml("key: &anchor value\n", &default_opts());
        assert_eq!(result, "key: &anchor value\n");
    }

    // K2: Anchor on a block mapping value is preserved.
    #[test]
    fn anchor_block_mapping_preserved() {
        let result = format_yaml("defaults: &defaults\n  timeout: 30\n", &default_opts());
        assert_eq!(result, "defaults: &defaults\n  timeout: 30\n");
    }

    // K3: Anchor on a block sequence value is preserved.
    #[test]
    fn anchor_block_sequence_preserved() {
        let result = format_yaml("items: &mylist\n  - a\n  - b\n", &default_opts());
        assert_eq!(result, "items: &mylist\n  - a\n  - b\n");
    }

    // K4: Anchor on a flow mapping value is preserved.
    #[test]
    fn anchor_flow_mapping_preserved() {
        let result = format_yaml("key: &anchor {a: 1}\n", &default_opts());
        assert!(result.contains("&anchor"), "anchor missing: {result:?}");
    }

    // K5: Anchor on a flow sequence value is preserved.
    #[test]
    fn anchor_flow_sequence_preserved() {
        let result = format_yaml("key: &anchor [a, b]\n", &default_opts());
        assert_eq!(result, "key: &anchor [a, b]\n");
    }

    // K6: Anchor on a block-mapping sequence item is preserved.
    #[test]
    fn anchor_sequence_item_block_mapping_preserved() {
        let result = format_yaml("items:\n  - &item\n    key: val\n", &default_opts());
        assert_eq!(result, "items:\n  - &item\n    key: val\n");
    }

    // K7: Alias reference (`*name`) is preserved (regression guard).
    #[test]
    fn alias_reference_preserved() {
        let result = format_yaml(
            "defaults: &defaults\n  timeout: 30\nservice:\n  <<: *defaults\n",
            &default_opts(),
        );
        assert!(result.contains("&defaults"), "anchor missing: {result:?}");
        assert!(result.contains("*defaults"), "alias missing: {result:?}");
    }

    // K8: Anchor+alias round-trip is idempotent.
    #[test]
    fn anchor_alias_idempotent() {
        let input = "defaults: &defaults\n  timeout: 30\nservice:\n  <<: *defaults\n";
        let first = format_yaml(input, &default_opts());
        let second = format_yaml(&first, &default_opts());
        assert_eq!(first, second, "anchor/alias not idempotent: {first:?}");
    }

    // AP-2: Anchor on a top-level plain scalar is preserved.
    #[test]
    fn anchor_on_top_level_scalar_preserved() {
        let result = format_yaml("&doc hello\n", &default_opts());
        assert_eq!(result, "&doc hello\n");
    }

    // AP-10: Anchor and alias round-trip on a sequence value.
    #[test]
    fn anchor_and_alias_round_trip_sequence() {
        let input = "base: &base\n  - x\n  - y\nextended:\n  - *base\n";
        let result = format_yaml(input, &default_opts());
        assert!(result.contains("&base"), "anchor missing: {result:?}");
        assert!(result.contains("- x"), "sequence item missing: {result:?}");
        assert!(result.contains("*base"), "alias missing: {result:?}");
    }

    // AP-12: Anchor and user tag on same scalar — anchor precedes tag (YAML spec §6.8.1).
    #[test]
    fn anchor_before_tag_on_scalar() {
        let result = format_yaml("item: &myanchor !mytag value\n", &default_opts());
        assert!(result.contains("&myanchor"), "anchor missing: {result:?}");
        assert!(result.contains("!mytag"), "tag missing: {result:?}");
        assert!(result.contains("value"), "value missing: {result:?}");
        // Anchor must precede tag in the output string (YAML spec §6.8.1).
        // Split on the tag: the prefix must contain the anchor.
        let before_tag = result.split("!mytag").next().unwrap_or("");
        assert!(
            before_tag.contains("&myanchor"),
            "anchor must precede tag per YAML spec §6.8.1: {result:?}"
        );
    }

    // AP-13: Anchor coexists with trailing inline comment.
    #[test]
    fn anchor_with_trailing_comment_preserved() {
        let result = format_yaml("key: &anchor value  # inline comment\n", &default_opts());
        assert!(
            result.contains("&anchor value"),
            "anchor+value missing: {result:?}"
        );
        assert!(
            result.contains("# inline comment"),
            "comment missing: {result:?}"
        );
    }

    // AP-14: Anchor on an empty flow mapping is preserved.
    #[test]
    fn anchor_on_empty_flow_mapping_preserved() {
        let result = format_yaml("empty: &empty {}\n", &default_opts());
        assert_eq!(result, "empty: &empty {}\n");
    }

    // AP-15: Anchor on an empty flow sequence is preserved.
    #[test]
    fn anchor_on_empty_flow_sequence_preserved() {
        let result = format_yaml("empty: &empty []\n", &default_opts());
        assert_eq!(result, "empty: &empty []\n");
    }

    // AP-16: No spurious `&` sigil is injected when no anchor is defined.
    #[test]
    fn no_spurious_anchor_when_none() {
        let result = format_yaml("key: value\n", &default_opts());
        assert!(
            !result.contains('&'),
            "spurious anchor in output: {result:?}"
        );
    }

    // ---- Group L: Document marker flag emission ----

    // FM-1 (spike): Single bare document — no `---` or `...` emitted
    #[test]
    fn bare_document_emits_no_markers() {
        let result = format_yaml("key: value\n", &default_opts());
        assert!(result.contains("key: value"), "content missing: {result:?}");
        assert!(
            !result.contains("---"),
            "unexpected `---` in output: {result:?}"
        );
        assert!(
            !result.contains("..."),
            "unexpected `...` in output: {result:?}"
        );
    }

    // FM-2: Single document with explicit `---` start → `---` preserved in output
    #[test]
    fn explicit_start_marker_preserved() {
        let result = format_yaml("---\nkey: value\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` missing from output: {result:?}"
        );
    }

    // FM-3: Single document with explicit `...` end → `...` preserved in output
    #[test]
    fn explicit_end_marker_preserved() {
        let result = format_yaml("key: value\n...\n", &default_opts());
        assert!(
            result.contains("..."),
            "`...` missing from output: {result:?}"
        );
    }

    // A comment after the root scalar must not hide the `...` that follows it
    #[rstest]
    #[case::comment_line_before_marker("v\n# d\n...\n")]
    #[case::indented_comment_line_before_marker("v\n  # d\n...\n")]
    #[case::trailing_comment_before_marker("v # c\n...\n")]
    fn explicit_end_marker_preserved_after_root_scalar_with_comment(#[case] input: &str) {
        let result = format_yaml(input, &default_opts());
        assert!(
            result.trim_end().ends_with("..."),
            "`...` missing from output: {result:?}"
        );
        assert_eq!(format_yaml(&result, &default_opts()), result);
    }

    // The loader stores a block scalar's header comment as its trailing
    // comment; the formatter prints it on the header line, never after the
    // scalar's content, where it would become part of the value.
    #[rstest]
    #[case::literal_value("a: | # c\n  t\nb: 1\n", "a: |  # c\n  t\nb: 1\n", "t\n")]
    #[case::folded_value("a: > # c\n  t\n", "a: >  # c\n  t\n", "t\n")]
    #[case::literal_item("- | # c\n  t\n- x\n", "- |  # c\n  t\n- x\n", "t\n")]
    #[case::literal_explicit_key_value("? k\n: | # c\n  t\n", "k: |  # c\n  t\n", "t\n")]
    fn block_scalar_header_comment_stays_in_the_header(
        #[case] input: &str,
        #[case] expected: &str,
        #[case] value: &str,
    ) {
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        assert!(
            scalar_values(&output).iter().any(|v| v == value),
            "the value must stay {value:?}: {output:?}"
        );
    }

    // FM-4: Single document with both `---` and `...` → both preserved
    #[test]
    fn both_markers_preserved() {
        let result = format_yaml("---\nkey: value\n...\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` missing from output: {result:?}"
        );
        assert!(
            result.contains("..."),
            "`...` missing from output: {result:?}"
        );
    }

    // FM-5: Multi-document — `---` separator always emitted between docs
    #[test]
    fn multi_document_separator_always_emitted() {
        let result = format_yaml("doc1: a\n---\ndoc2: b\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` separator missing: {result:?}"
        );
        assert!(
            result.contains("doc1: a"),
            "doc1 content missing: {result:?}"
        );
        assert!(
            result.contains("doc2: b"),
            "doc2 content missing: {result:?}"
        );
    }

    // FM-6: Multi-document — `...` terminator on first doc only, not second
    #[test]
    fn explicit_end_only_on_first_document() {
        let result = format_yaml("doc1: a\n...\n---\ndoc2: b\n", &default_opts());
        assert!(
            result.contains("---"),
            "`---` separator missing: {result:?}"
        );
        // The full output should be exactly: doc1 content, ..., ---, doc2 content.
        // The `...` appears before `doc2: b`, not after it.
        assert!(
            result.contains("..."),
            "`...` missing from output: {result:?}"
        );
        assert!(
            result.find("...") < result.find("doc2: b"),
            "`...` should appear before doc2, got: {result:?}"
        );
        // The portion after `doc2: b` must not contain `...`.
        let after_doc2 = result.find("doc2: b").map_or("", |pos| &result[pos..]);
        assert!(
            !after_doc2.contains("..."),
            "unexpected `...` after doc2: {result:?}"
        );
    }

    // FM-7: Multi-document — `...` on all documents → all preserved
    #[test]
    fn explicit_end_on_all_documents_preserved() {
        let result = format_yaml("doc1: a\n...\n---\ndoc2: b\n...\n", &default_opts());
        // Both `...` markers should be present
        let count = result.matches("...").count();
        assert_eq!(
            count, 2,
            "expected 2 `...` markers, got {count}: {result:?}"
        );
    }

    // ---- Group: multi-line plain scalar followed by a comment line ----

    #[rstest]
    #[case::before_next_key("a:\n  v\n  w\n# d\nb: 1\n", "a: v w\n# d\nb: 1\n")]
    #[case::at_end_of_input("a:\n  v\n  w\n# d\n", "a: v w\n# d\n")]
    fn comment_line_after_multiline_plain_scalar_is_stable(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let first = format_yaml(input, &default_opts());
        assert_eq!(first, expected);
        assert_eq!(format_yaml(&first, &default_opts()), first);
    }

    // ---- Group: document-level comments ----

    /// The comments around each document's root as the loader stores them
    /// (before `---`, after `---`, after the root, after `...`); a stream
    /// without a document yields its comment events as one entry.
    fn document_comment_lists(yaml: &str) -> Vec<[Vec<String>; 4]> {
        let docs = rlsp_yaml_parser::load(yaml).expect("test input must parse");
        if docs.is_empty() {
            let comments = stream_comments(yaml).expect("test input must parse");
            return vec![[comments, Vec::new(), Vec::new(), Vec::new()]];
        }
        docs.into_iter()
            .map(|d| {
                [
                    d.prefix_comments,
                    d.comments,
                    d.end_comments,
                    d.after_end_comments,
                ]
            })
            .collect()
    }

    /// `output` is a fixed point of `format_yaml` and holds the comments of
    /// `yaml` in the same lists.
    fn assert_comments_survive(yaml: &str, output: &str, opts: &YamlFormatOptions) {
        assert_eq!(format_yaml(output, opts), output, "output is not stable");
        assert_eq!(
            document_comment_lists(output),
            document_comment_lists(yaml),
            "comments differ between input {yaml:?} and output {output:?}"
        );
    }

    fn assert_formats_to(yaml: &str, expected: &str) {
        let output = format_yaml(yaml, &default_opts());
        assert_eq!(output, expected);
        assert_comments_survive(yaml, &output, &default_opts());
    }

    #[rstest]
    #[case::own_line_after_marker("---\n# c\na: 1\n", "---\n# c\na: 1\n")]
    #[case::prefix_before_marker("# c\n---\na: 1\n", "# c\n---\na: 1\n")]
    #[case::end_comment_before_marker("a: v\n# d\n---\nb: 1\n", "a: v\n# d\n---\nb: 1\n")]
    #[case::root_scalar_trailing_comment_before_marker("v # c\n---\nw\n", "v  # c\n---\nw\n")]
    #[case::end_and_prefix_around_document_end(
        "a: 1\n# d\n...\n# e\n---\nb: 2\n",
        "a: 1\n# d\n...\n# e\n---\nb: 2\n"
    )]
    #[case::marker_line_comment_moves_below_marker("--- # c\na: 1\n", "---\n# c\na: 1\n")]
    #[case::directive_line_comment_becomes_prefix("%YAML 1.2 # c\n---\na: 1\n", "# c\n---\na: 1\n")]
    #[case::document_end_line_comment_moves_below("a: 1\n... # c\n", "a: 1\n...\n# c\n")]
    #[case::comment_only_stream("# only\n", "# only\n")]
    #[case::implicit_second_document_keeps_prefix_before_inserted_marker(
        "a: 1\n...\n# e\nb: 2\n",
        "a: 1\n...\n# e\n---\nb: 2\n"
    )]
    #[case::end_comment_without_document_end(
        "a: 1\n# d\n---\nb: 1\n# e\n",
        "a: 1\n# d\n---\nb: 1\n# e\n"
    )]
    #[case::comment_after_marker_of_second_document(
        "a: 1\n---\n# x\nb: 2\n",
        "a: 1\n---\n# x\nb: 2\n"
    )]
    #[case::after_end_comments_keep_order("a: 1\n...\n# e1\n# e2\n", "a: 1\n...\n# e1\n# e2\n")]
    #[case::nested_tail_leaves_its_indent("a:\n  b: 1\n  # tail\n", "a:\n  b: 1\n# tail\n")]
    #[case::sequence_tail("- a\n- b\n# tail\n", "- a\n- b\n# tail\n")]
    #[case::comment_before_last_scalar_value_moves_after_root("a:\n  # c\n  v\n", "a: v\n# c\n")]
    #[case::root_scalar_end_comment("v\n# c\n", "v\n# c\n")]
    #[case::key_value_then_comment_line("key: value\n# next\n", "key: value\n# next\n")]
    fn document_comment_is_printed_at_its_place(#[case] input: &str, #[case] expected: &str) {
        assert_formats_to(input, expected);
    }

    #[rstest]
    #[case::comment_under_empty_value("a:\n  # c\n")]
    #[case::comment_only_with_document_end("# c\n...\n")]
    #[case::empty_documents_with_comments("# a\n---\n# b\n---\n# c\n")]
    #[case::comment_after_end_of_empty_document("---\n...\n# e\n")]
    #[case::comment_only_across_markers("# a\n---\n# b\n")]
    #[case::keep_chomp_blank_then_end_comment("a: |+\n  t\n\n# d\n")]
    #[case::comment_with_nel_stays_one_line("# a\u{85}b\na: 1\n")]
    #[case::comment_looks_like_yaml("# ---\n# ...\n# - x\n# a: \"b\na: 1\n")]
    #[case::comment_with_unbalanced_quotes("# it's \"odd\na: 1\n")]
    #[case::comment_with_inner_hash_unicode_and_trailing_spaces("# a # b ü 日本  \n---\na: 1\n")]
    #[case::hash_inside_plain_scalar_then_real_comment("a: b#c  # d\n")]
    #[case::multiline_double_quoted_hash_line_is_no_comment("a: \"x\n  # y\"\nb: 1\n")]
    fn document_comment_survives_formatting(#[case] input: &str) {
        let output = format_yaml(input, &default_opts());
        assert_comments_survive(input, &output, &default_opts());
    }

    #[rstest]
    #[case::unchanged_single_line_comment("# c\na: 1\n")]
    #[case::unchanged_nel_comment("# a\u{85}b\na: 1\n")]
    #[case::unchanged_modeline_like_comment("# yaml-language-server: $schema=x\na: 1\n")]
    #[case::unchanged_comment_without_space("#nospace\na: 1\n")]
    #[case::unchanged_comment_with_two_spaces("#  two\na: 1\n")]
    #[case::unchanged_comment_with_trailing_spaces("# t  \na: 1\n")]
    #[case::unchanged_comment_with_inner_hash_and_unicode("# a # b ü 日本\na: 1\n")]
    #[case::unchanged_unbalanced_quotes("# it's \"odd\na: 1\n")]
    #[case::unchanged_yaml_looking_text("# ---\n# ...\n# - x\na: 1\n")]
    #[case::unchanged_comment_without_text("#\na: 1\n")]
    fn comment_text_is_printed_verbatim(#[case] input: &str) {
        assert_formats_to(input, input);
    }

    #[test]
    fn comment_longer_than_print_width_is_not_wrapped() {
        let comment = format!("# {}", "word ".repeat(30));
        let input = format!("{comment}\n---\n{comment}\na: 1\n{comment}\n...\n{comment}\n");
        for print_width in [20, 80] {
            let opts = YamlFormatOptions {
                print_width,
                ..default_opts()
            };
            assert_eq!(format_yaml(&input, &opts), input);
        }
    }

    #[test]
    fn modeline_after_marker_stays_a_line_of_its_own() {
        let output = format_yaml(
            "--- # yaml-language-server: $schema=x\na: 1\n",
            &default_opts(),
        );
        assert_eq!(output, "---\n# yaml-language-server: $schema=x\na: 1\n");
    }

    #[test]
    fn comments_keep_source_order_across_document_lists() {
        let input = "# one\n---\n# two\na: 1\n# three\n...\n# four\n---\n# five\nb: 2\n# six\n...\n# seven\n";
        assert_formats_to(input, input);
    }

    #[test]
    fn modeline_written_after_marker_acts_as_modeline_in_the_output() {
        let input = "--- # yaml-language-server: $schema=x\na: 1\n";
        assert_eq!(crate::schema::association::extract_schema_url(input), None);
        let output = format_yaml(input, &default_opts());
        assert_eq!(
            crate::schema::association::extract_schema_url(&output),
            Some("x".to_string())
        );
    }

    #[test]
    fn modeline_on_its_own_line_keeps_its_line() {
        let input = "# yaml-language-server: $schema=x\n---\na: 1\n";
        assert_formats_to(input, input);
        let input = "---\n# yaml-language-server: $schema=x\na: 1\n";
        assert_formats_to(input, input);
    }

    #[test]
    fn first_of_several_modelines_stays_first_across_document_lists() {
        let input = "# yaml-language-server: $schema=first\n---\n# yaml-language-server: $schema=second\na: 1\n# yaml-language-server: $schema=third\n";
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, input);
        assert_eq!(
            crate::schema::association::extract_schema_url(&output),
            Some("first".to_string())
        );
    }

    #[rstest]
    #[case::directly_above_key(
        "a: 1\n# rlsp-yaml-disable-next-line\nb: 2\n",
        "a: 1\n# rlsp-yaml-disable-next-line\nb: 2\n"
    )]
    #[case::blank_line_before_comment_is_kept(
        "a: 1\n\n# rlsp-yaml-disable-next-line\nb: 2\n",
        "a: 1\n\n# rlsp-yaml-disable-next-line\nb: 2\n"
    )]
    #[case::crlf_input(
        "a: 1\r\n\r\n# rlsp-yaml-disable-next-line\r\nb: 2\r\n",
        "a: 1\n\n# rlsp-yaml-disable-next-line\nb: 2\n"
    )]
    #[case::after_changed_line(
        "a: \"q\"\n# rlsp-yaml-disable-next-line\nb: 2\n",
        "a: q\n# rlsp-yaml-disable-next-line\nb: 2\n"
    )]
    fn suppression_comment_stays_directly_above_its_line(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        let target = output
            .lines()
            .position(|l| l == "b: 2")
            .expect("b: 2 is in the output");
        let suppression = crate::validation::suppression::build_suppression_map(&output);
        assert!(suppression.is_suppressed(u32::try_from(target).expect("fits"), "any"));
    }

    #[test]
    fn marker_line_suppression_comments_become_active_on_their_own_line() {
        let output = format_yaml("--- # rlsp-yaml-disable-file\na: 1\n", &default_opts());
        assert_eq!(output, "---\n# rlsp-yaml-disable-file\na: 1\n");
        let suppression = crate::validation::suppression::build_suppression_map(&output);
        assert!(suppression.is_suppressed(0, "any"));
    }

    #[rstest]
    #[case::apostrophe_in_plain_scalar("a: don't # c\n", "a: don't  # c\n")]
    #[case::single_quoted_with_escaped_quote("k: 'it''s' # c\n", "k: it's  # c\n")]
    #[case::hash_inside_double_quotes("a: \"x # y\" # c\n", "a: \"x # y\"  # c\n")]
    #[case::blank_lines_and_hash_line_in_literal(
        "a: |\n  x\n\n  # y\n\n  z\n# e\n",
        "a: |\n  x\n\n  # y\n\n  z\n# e\n"
    )]
    fn comment_like_text_in_scalars_stays_in_the_scalar(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        assert_formats_to(input, expected);
        let values = |text: &str| -> Vec<String> {
            let docs = rlsp_yaml_parser::load(text).expect("test input must parse");
            let Node::Mapping { entries, .. } = &docs[0].root else {
                panic!("test input must be a mapping");
            };
            entries
                .iter()
                .filter_map(|(_, v)| match v {
                    Node::Scalar { value, .. } => Some(value.clone()),
                    Node::Mapping { .. } | Node::Sequence { .. } | Node::Alias { .. } => None,
                })
                .collect()
        };
        assert_eq!(values(&format_yaml(input, &default_opts())), values(input));
    }

    #[test]
    fn comment_above_each_key_is_kept() {
        assert_formats_to(
            "# first\nname: \"P\"\n# second\non: x\n",
            "# first\nname: P\n# second\non: x\n",
        );
    }

    #[test]
    fn hash_line_at_end_of_block_scalar_is_content_not_a_comment() {
        assert_formats_to("a: |\n  t\n  # d\n", "a: |\n  t\n  # d\n");
    }

    #[rstest]
    #[case::hash_line_at_start("a: |\n  # d\n  t\n")]
    #[case::content_hash_line_then_end_comment("a: |\n  t\n  # d\n# e\n")]
    #[case::column_zero_comment_after_scalar("a: |\n  t\n# d\n")]
    #[case::column_zero_comment_before_next_key("a: |\n  t\n# d\nb: 1\n")]
    #[case::keep_chomp_comment_before_next_key("a: |+\n  t\n\n# d\nb: 1\n")]
    fn block_scalar_hash_lines_are_not_counted_as_comments(#[case] input: &str) {
        assert_formats_to(input, input);
    }

    #[rstest]
    #[case::header_comment_and_blank_line_kept("# h\n\na: 1\n", "# h\n\na: 1\n")]
    #[case::blank_lines_between_comment_groups_collapse(
        "# a\n\n\n# b\n\na: 1\n",
        "# a\n\n# b\n\na: 1\n"
    )]
    #[case::blank_line_before_end_comment("a: 1\n\n# d\n---\nb: 2\n", "a: 1\n\n# d\n---\nb: 2\n")]
    #[case::blank_line_between_end_comment_and_marker(
        "a: 1\n# d\n\n---\nb: 2\n",
        "a: 1\n# d\n\n---\nb: 2\n"
    )]
    #[case::blank_lines_around_document_end_comments(
        "a: 1\n\n# d\n...\n\n# e\n",
        "a: 1\n\n# d\n...\n\n# e\n"
    )]
    #[case::comment_only_stream_keeps_blank_line("# a\n\n# b\n", "# a\n\n# b\n")]
    #[case::comment_only_stream_drops_leading_blank_lines("\n\n# a\n", "# a\n")]
    #[case::comment_only_stream_drops_trailing_blank_lines("# a\n\n\n", "# a\n")]
    #[case::comment_only_stream_drops_indent("   # ind\n#nospace\n#\n", "# ind\n#nospace\n#\n")]
    fn blank_lines_around_document_comments_are_restored(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        assert_formats_to(input, expected);
    }

    #[rstest]
    #[case::empty("")]
    #[case::newline("\n")]
    #[case::spaces("   \n")]
    #[case::document_end_marker("...\n")]
    fn input_without_comment_formats_to_empty(#[case] input: &str) {
        assert_eq!(format_yaml(input, &default_opts()), "");
    }

    #[rstest]
    #[case::empty_root("---\n", "---\n")]
    #[case::empty_root_after_document("a: 1\n---\n", "a: 1\n---\n")]
    #[case::comment_after_empty_root_marker("--- # c\n", "---\n# c\n")]
    #[case::anchored_empty_root("--- &a\n", "---\n&a\n")]
    #[case::tagged_empty_root("--- !t\n", "---\n!t\n")]
    #[case::core_tagged_empty_root("--- !!null\n", "---\n!!null\n")]
    #[case::quoted_empty_root("--- \"\"\n", "---\n\"\"\n")]
    #[case::anchored_and_tagged_empty_root("--- &a !t\n", "---\n&a !t\n")]
    fn last_document_prints_its_root_unless_it_prints_nothing(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        for single_quote in [false, true] {
            let opts = YamlFormatOptions {
                single_quote,
                ..default_opts()
            };
            assert_eq!(format_yaml(input, &opts), expected);
        }
    }

    #[test]
    fn comment_only_stream_with_unprintable_character_is_returned_unchanged() {
        let input = "# a\n# \u{1}\n";
        assert_eq!(format_yaml(input, &default_opts()), input);
    }

    #[rstest]
    #[case::comment_only("# a\r\n# b\r\n", "# a\n# b\n")]
    #[case::document_level_comments(
        "a: 1\r\n# d\r\n---\r\nb: 2\r\n# e\r\n",
        "a: 1\n# d\n---\nb: 2\n# e\n"
    )]
    #[case::prefix_and_marker_comments("# c\r\n---\r\n# m\r\na: 1\r\n", "# c\n---\n# m\na: 1\n")]
    #[case::lone_cr_line_endings("# a\r\r# b\ra: 1\r", "# a\n\n# b\na: 1\n")]
    fn crlf_input_prints_document_comments_without_carriage_returns(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        assert!(!output.contains('\r'));
    }

    #[test]
    fn comment_only_stream_uses_the_requested_line_ending() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Crlf,
            ..default_opts()
        };
        assert_eq!(format_yaml("# a\n\n# b\n", &opts), "# a\r\n\r\n# b\r\n");
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Cr,
            ..default_opts()
        };
        assert_eq!(format_yaml("# a\n# b\n", &opts), "# a\r# b\r");
    }

    #[test]
    fn comment_only_stream_without_final_newline() {
        let opts = YamlFormatOptions {
            insert_final_newline: false,
            ..default_opts()
        };
        assert_eq!(format_yaml("# only\n", &opts), "# only");
    }

    #[test]
    fn document_comments_do_not_depend_on_style_options() {
        let input =
            "# p\n---\n# m\na: \"x\"\n# e\n...\n# z\n---\n# m2\nb: [1, 2]\n# e2\n...\n# after\n";
        let expected = format_yaml(input, &default_opts());
        for opts in [
            YamlFormatOptions {
                print_width: 20,
                ..default_opts()
            },
            YamlFormatOptions {
                format_enforce_block_style: true,
                ..default_opts()
            },
            YamlFormatOptions {
                preserve_quotes: true,
                ..default_opts()
            },
            YamlFormatOptions {
                single_quote: true,
                ..default_opts()
            },
        ] {
            let output = format_yaml(input, &opts);
            assert_eq!(
                document_comment_lists(&output),
                document_comment_lists(input)
            );
            assert_eq!(output.lines().filter(|l| l.starts_with('#')).count(), 7);
            assert_eq!(expected.lines().filter(|l| l.starts_with('#')).count(), 7);
        }
    }

    #[test]
    fn duplicate_key_removal_leaves_document_comments_in_place() {
        let opts = YamlFormatOptions {
            format_remove_duplicate_keys: true,
            ..default_opts()
        };
        let output = format_yaml("# p\n---\n# m\na: 1\na: 2\n# e\n...\n# z\n", &opts);
        assert_eq!(output, "# p\n---\n# m\na: 2\n# e\n...\n# z\n");
    }

    #[test]
    fn many_comment_lines_and_documents_format_without_blowup() {
        let comments = "# c\n".repeat(20_000);
        assert_eq!(format_yaml(&comments, &default_opts()), comments);
        let documents = "# c\n---\na: 1\n".repeat(20_000);
        let output = format_yaml(&documents, &default_opts());
        assert_eq!(output.lines().filter(|l| *l == "# c").count(), 20_000);
    }

    // ---- Group: blank lines never change a scalar value ----

    /// Every scalar value of `yaml`, keys included, in document order.
    fn scalar_values(yaml: &str) -> Vec<String> {
        fn collect(node: &Node<Span>, out: &mut Vec<String>) {
            match node {
                Node::Scalar { value, .. } => out.push(value.clone()),
                Node::Alias { name, .. } => out.push(format!("*{name}")),
                Node::Mapping { entries, .. } => {
                    for (key, value) in entries {
                        collect(key, out);
                        collect(value, out);
                    }
                }
                Node::Sequence { items, .. } => {
                    for item in items {
                        collect(item, out);
                    }
                }
            }
        }
        let mut out = Vec::new();
        for doc in rlsp_yaml_parser::load(yaml).expect("test input must parse") {
            collect(&doc.root, &mut out);
        }
        out
    }

    #[rstest]
    #[case::literal_line_equals_changed_line(
        "a: 1\n\nb: \"y\"\nc: |\n  b: \"y\"\n  z\n",
        "a: 1\nb: y\nc: |\n  b: \"y\"\n  z\n"
    )]
    #[case::folded_line_equals_changed_line(
        "a: 1\n\nb: \"y\"\nc: >\n  b: \"y\"\n",
        "a: 1\nb: y\nc: >\n  b: \"y\"\n"
    )]
    #[case::literal_line_equals_changed_sequence_item(
        "- 1\n\n- \"x\"\n- |\n  - \"x\"\n  z\n",
        "- 1\n- x\n- |\n  - \"x\"\n  z\n"
    )]
    #[case::literal_in_second_literal(
        "a: |\n  p: \"y\"\n  q\n\nb: |\n  r\n  p: \"y\"\n",
        "a: |\n  p: \"y\"\n  q\n\nb: |\n  r\n  p: \"y\"\n"
    )]
    #[case::multiline_double_quoted_line_equals_changed_line(
        "a: 1\n\nb: \"y\"\nc: \"p\n\n  b: \\\"y\\\"\"\n",
        "a: 1\nb: y\nc: \"p\\nb: \\\"y\\\"\"\n"
    )]
    #[case::empty_keep_literal_after_space_only_line(
        "c: |+\n  x\n      \nc: |+\nc: |+\n",
        "c: \"x\\n    \\n\"\nc: |+\nc: |+\n"
    )]
    #[case::empty_keep_literal_after_tab_only_line(
        "- |+\n  \t\n- |+\n- |+\n  q\n",
        "- \" \\t\\n\"\n- |+\n- |+\n  q\n"
    )]
    #[case::folded_leading_blank_line("k: >\n\n  x y\n", "k: >\n\n  x y\n")]
    #[case::folded_strip_leading_blank_line("k: >-\n\n  z\n", "k: >-\n\n  z\n")]
    #[case::folded_leading_and_inner_blank_line("k: >\n\n  x\n\n  y\n", "k: >\n\n  x\n\n  y\n")]
    #[case::folded_keep_trailing_blank_line_before_comment(
        "b: 1\nk: >+\n  z\n\n# c\n",
        "b: 1\nk: >+\n  z\n\n# c\n"
    )]
    #[case::folded_keep_trailing_blank_line_before_key(
        "k: >+\n  z\n\nb: 1\n",
        "k: >+\n  z\n\nb: 1\n"
    )]
    #[case::two_blank_lines_inside_literal("a: |\n  x\n\n\n  y\n", "a: |\n  x\n\n\n  y\n")]
    #[case::two_leading_blank_lines_in_literal("a: |\n\n\n  x\n", "a: |\n\n\n  x\n")]
    #[case::keep_chomp_trailing_blank_lines_before_next_key(
        "a: |+\n  x\n\n\nb: 1\n",
        "a: |+\n  x\n\n\nb: 1\n"
    )]
    #[case::keep_chomp_trailing_blank_lines_at_end_of_input("a: |+\n  x\n\n", "a: |+\n  x\n\n")]
    #[case::keep_chomp_blank_lines_only("- |+\n\n\n", "- |+\n\n\n")]
    #[case::blank_line_after_clip_literal_separates_it_from_the_next_key(
        "a: |\n  x\n\n\nb: 1\n",
        "a: |\n  x\n\nb: 1\n"
    )]
    #[case::tab_only_line_inside_literal("a: |\n  x\n  \t  \n  y\n", "a: |\n  x\n  \t  \n  y\n")]
    #[case::nested_literal_blank_lines_carry_no_indentation(
        "a:\n  b:\n    - |\n      x\n\n\n      y\n    - |+\n      p\n\n\n  c: 1\n",
        "a:\n  b:\n    - |\n      x\n\n\n      y\n    - |+\n      p\n\n\n  c: 1\n"
    )]
    #[case::nested_folded_blank_line_carries_no_indentation(
        "k:\n  - >\n    t\n\n    u\n",
        "k:\n  - >\n    t\n\n    u\n"
    )]
    fn blank_line_restoration_does_not_enter_a_scalar(#[case] input: &str, #[case] expected: &str) {
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        assert_eq!(scalar_values(&output), scalar_values(input));
    }

    #[test]
    fn blank_line_inside_literal_is_restored_only_in_the_same_literal() {
        let input = "a: |\n  x\n\n  y\nb: |\n  x\n  y\n";
        assert_formats_to(input, input);
        assert_eq!(
            scalar_values(&format_yaml(input, &default_opts())),
            scalar_values(input)
        );
    }

    #[rstest]
    #[case::top_level("a: |\n  x\n     \n  y\n", "a: \"x\\n   \\ny\\n\"\n")]
    #[case::nested_in_sequence(
        "k:\n  - |\n    x\n       \n    y\n",
        "k:\n  - \"x\\n   \\ny\\n\"\n"
    )]
    #[case::keep_chomp("a: |+\n  x\n   \n", "a: \"x\\n \\n\"\n")]
    fn space_only_value_line_of_a_literal_keeps_its_value(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        // The value line of spaces is no blank line to empty: the scalar is
        // printed double-quoted, so no output line consists of spaces alone.
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        assert_eq!(scalar_values(&output), scalar_values(input));
    }

    #[test]
    fn tab_line_of_a_literal_inside_a_sequence_keeps_its_value() {
        let input = "k:\n  - |\n    x\n    \t  \n    y\n";
        assert_formats_to(input, input);
        assert_eq!(
            scalar_values(&format_yaml(input, &default_opts())),
            scalar_values(input)
        );
    }

    #[test]
    fn literal_blank_lines_survive_removal_of_a_duplicate_key() {
        let opts = YamlFormatOptions {
            format_remove_duplicate_keys: true,
            ..default_opts()
        };
        let output = format_yaml("a: |\n  x\n\n  y\nb: |\n  p\nb: 2\n", &opts);
        assert_eq!(output, "a: |\n  x\n\n  y\nb: 2\n");
    }

    /// A unit of a generated top-level mapping.
    #[derive(Debug, Clone)]
    enum Unit {
        Line(&'static str),
        /// A literal scalar; each content line has 0 to 2 blank lines before it.
        Literal {
            header: &'static str,
            lines: Vec<(usize, &'static str)>,
        },
        /// A folded scalar; each content line has 0 to 2 blank lines before it.
        Folded {
            header: &'static str,
            lines: Vec<(usize, &'static str)>,
        },
    }

    /// Content lines that look like the structure lines of the mapping.
    const BLOCK_LINES: [&str; 6] = ["b: \"y\"", "- \"x\"", "z", "# c", "a: 1", "c: 'q'"];

    /// Content lines of a literal: the above, and lines of spaces or a tab alone.
    const LITERAL_LINES: [&str; 8] = [
        "b: \"y\"", "- \"x\"", "z", "# c", "a: 1", "c: 'q'", "    ", "\t",
    ];

    fn unit_strategy() -> impl Strategy<Value = Unit> {
        let line = prop::sample::select(BLOCK_LINES.to_vec());
        let literal_line = prop::sample::select(LITERAL_LINES.to_vec());
        prop_oneof![
            prop::sample::select(vec![
                "a: 1",
                "b: \"y\"",
                "c: 'q'",
                "d: [1, 2]",
                "e: z",
                "# c",
            ])
            .prop_map(Unit::Line),
            (
                prop::sample::select(vec!["k: |", "k: |-", "k: |+"]),
                prop::collection::vec((0usize..=2, literal_line), 0..=3),
            )
                .prop_map(|(header, lines)| Unit::Literal { header, lines }),
            (
                prop::sample::select(vec!["k: >", "k: >-", "k: >+"]),
                prop::collection::vec((0usize..=2, line), 0..=3),
            )
                .prop_map(|(header, lines)| Unit::Folded { header, lines }),
        ]
    }

    /// One blank line between units, or none; blank lines inside block scalars.
    fn render_units(units: &[(Unit, bool)]) -> String {
        let mut text = String::new();
        for (unit, blank_before) in units {
            if *blank_before {
                text.push('\n');
            }
            match unit {
                Unit::Line(line) => {
                    text.push_str(line);
                    text.push('\n');
                }
                Unit::Literal { header, lines } | Unit::Folded { header, lines } => {
                    text.push_str(header);
                    text.push('\n');
                    for (blanks, line) in lines {
                        text.push_str(&"\n".repeat(*blanks));
                        text.push_str("  ");
                        text.push_str(line);
                        text.push('\n');
                    }
                }
            }
        }
        text
    }

    #[test]
    fn formatting_never_changes_a_scalar_value_across_blank_lines() {
        let mut runner = TestRunner::new_with_rng(
            Config {
                cases: 512,
                max_global_rejects: 8192,
                failure_persistence: None,
                ..Config::default()
            },
            TestRng::from_seed(RngAlgorithm::ChaCha, &[7; 32]),
        );
        let strategy = prop::collection::vec((unit_strategy(), any::<bool>()), 1..=8);
        let result = runner.run(&strategy, |units| {
            let input = render_units(&units);
            if rlsp_yaml_parser::load(&input).is_err() {
                return Err(TestCaseError::reject("generated input does not load"));
            }
            let output = format_yaml(&input, &default_opts());
            prop_assert_eq!(
                scalar_values(&output),
                scalar_values(&input),
                "input {:?}",
                input
            );
            prop_assert_eq!(format_yaml(&output, &default_opts()), output);
            Ok(())
        });
        result.expect("formatting changed a scalar value or was not stable");
    }

    // ---- Group FS: format_subtree unit tests ----

    fn parse_root(src: &str) -> Node<Span> {
        rlsp_yaml_parser::load(src)
            .expect("test input must parse")
            .remove(0)
            .root
    }

    // FS-1: scalar node, base_indent 0 — single line, no indent applied
    #[test]
    fn format_subtree_scalar_base_indent_zero() {
        let node = parse_root("hello");
        let result = format_subtree(&node, &default_opts(), 0);
        assert_eq!(result, "hello");
    }

    // FS-2: scalar node, base_indent 4 — first line never indented
    #[test]
    fn format_subtree_scalar_base_indent_never_indents_first_line() {
        let node = parse_root("hello");
        let result = format_subtree(&node, &default_opts(), 4);
        assert_eq!(result, "hello");
    }

    // FS-3: empty mapping emits `{}` — records mapping_to_doc short-circuit
    #[test]
    fn format_subtree_empty_mapping_emits_inline() {
        let node = parse_root("{}");
        let result = format_subtree(&node, &default_opts(), 0);
        assert_eq!(result, "{}");
    }

    // FS-4: empty sequence emits `[]` — records sequence_to_doc short-circuit
    #[test]
    fn format_subtree_empty_sequence_emits_inline() {
        let node = parse_root("[]");
        let result = format_subtree(&node, &default_opts(), 0);
        assert_eq!(result, "[]");
    }

    // FS-5 through FS-7: block mapping with various base_indent values
    #[rstest]
    #[case::indent_zero(0, "a: 1", "b: 2")]
    #[case::indent_two(2, "a: 1", "  b: 2")]
    #[case::indent_eight(8, "a: 1", "        b: 2")]
    fn format_subtree_block_mapping_base_indent(
        #[case] base_indent: usize,
        #[case] expected_line0: &str,
        #[case] expected_line1: &str,
    ) {
        let node = parse_root("a: 1\nb: 2\n");
        let result = format_subtree(&node, &default_opts(), base_indent);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, expected_line0, "line 0 mismatch: {result:?}");
                assert_eq!(*line1, expected_line1, "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-8: block sequence, base_indent 2
    #[test]
    fn format_subtree_block_sequence_continuation_indented() {
        let node = parse_root("- x\n- y\n");
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "- x", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  - y", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-9: nested mapping inside sequence, base_indent 2
    #[test]
    fn format_subtree_nested_mapping_in_sequence_base_indent() {
        let node = parse_root("- a: 1\n  b: 2\n- c: 3\n");
        let result = format_subtree(&node, &default_opts(), 2);
        let lines: Vec<&str> = result.lines().collect();
        // First line has no leading spaces regardless of base_indent
        let first = lines.first().expect("output must have at least one line");
        assert!(
            first.starts_with("- a: 1"),
            "first line should start with `- a: 1`: {result:?}"
        );
        // The `- c: 3` item must have two leading spaces from base_indent
        let c_line = lines
            .iter()
            .find(|l| l.contains("c: 3"))
            .copied()
            .expect("output must contain `c: 3`");
        assert!(
            c_line.starts_with("  - c: 3"),
            "`- c: 3` line should have two leading spaces: {result:?}"
        );
    }

    // FS-10: enforce_block_style option converts flow mapping to block (tests the options-flag path)
    #[test]
    fn format_subtree_enforce_block_style_option_converts_flow_to_block() {
        let node = parse_root("{a: 1, b: 2}");
        let opts = YamlFormatOptions {
            format_enforce_block_style: true,
            ..YamlFormatOptions::default()
        };
        let result = format_subtree(&node, &opts, 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "a: 1", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  b: 2", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-11: flow mapping node → block via direct style mutation (Task 2 mechanism), base_indent 2
    #[test]
    fn format_subtree_flow_mapping_style_mutation_to_block() {
        let mut node = parse_root("{a: 1, b: 2}");
        if let Node::Mapping { style, .. } = &mut node {
            *style = CollectionStyle::Block;
        }
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "a: 1", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  b: 2", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // FS-12: flow sequence node → block via direct style mutation, base_indent 2
    #[test]
    fn format_subtree_flow_sequence_style_mutation_to_block() {
        let mut node = parse_root("[a, b, c]");
        if let Node::Sequence { style, .. } = &mut node {
            *style = CollectionStyle::Block;
        }
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, line2, ..] => {
                assert_eq!(*line0, "- a", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  - b", "line 1 mismatch: {result:?}");
                assert_eq!(*line2, "  - c", "line 2 mismatch: {result:?}");
            }
            other => panic!("expected at least 3 lines, got: {other:?}"),
        }
    }

    // FS-13: nested flow mappings inside a flow sequence, both flipped to block via style mutation
    #[test]
    fn format_subtree_nested_flow_in_flow_sequence_to_block() {
        let mut node = parse_root("[{a: 1}, {b: 2}]");
        // Flip outer sequence and each inner mapping to Block — mimics Task 2's approach
        if let Node::Sequence { style, items, .. } = &mut node {
            *style = CollectionStyle::Block;
            for item in items.iter_mut() {
                if let Node::Mapping { style: ms, .. } = item {
                    *ms = CollectionStyle::Block;
                }
            }
        }
        let result = format_subtree(&node, &default_opts(), 2);
        // Each sequence item becomes a `- key: val` block entry
        assert!(result.contains("a: 1"), "a: 1 missing: {result:?}");
        assert!(result.contains("b: 2"), "b: 2 missing: {result:?}");
        // Continuation lines must have 2 leading spaces
        let second_item_line = result
            .lines()
            .find(|l| l.contains("b: 2"))
            .expect("line with b: 2 must exist");
        assert!(
            second_item_line.starts_with("  "),
            "second item line must be indented by 2: {result:?}"
        );
    }

    // FS-14: multi-line flow mapping input converted to block via style mutation
    #[test]
    fn format_subtree_multiline_flow_mapping_to_block() {
        let mut node = parse_root("{\n  a: 1,\n  b: 2,\n}");
        if let Node::Mapping { style, .. } = &mut node {
            *style = CollectionStyle::Block;
        }
        let result = format_subtree(&node, &default_opts(), 2);
        match result.lines().collect::<Vec<_>>().as_slice() {
            [line0, line1, ..] => {
                assert_eq!(*line0, "a: 1", "line 0 mismatch: {result:?}");
                assert_eq!(*line1, "  b: 2", "line 1 mismatch: {result:?}");
            }
            other => panic!("expected at least 2 lines, got: {other:?}"),
        }
    }

    // ---- line_ending and insert_final_newline fields ------------------------

    // F1: LineEnding::Crlf replaces all LF with CRLF.
    #[test]
    fn line_ending_crlf_replaces_all_newlines() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Crlf,
            ..default_opts()
        };
        let output = format_yaml("a: 1\nb: 2\n", &opts);
        assert!(output.contains("\r\n"), "output should contain CRLF");
        for (i, ch) in output.char_indices() {
            if ch == '\n' {
                assert!(
                    i > 0 && output.as_bytes()[i - 1] == b'\r',
                    "bare LF at byte {i}"
                );
            }
        }
    }

    // F2: LineEnding::Cr replaces all LF with CR.
    #[test]
    fn line_ending_cr_replaces_all_newlines() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Cr,
            ..default_opts()
        };
        let output = format_yaml("a: 1\nb: 2\n", &opts);
        assert!(!output.contains('\n'), "output should have no LF");
        assert!(output.contains('\r'), "output should have at least one CR");
        assert!(!output.contains("\r\n"), "output should have no CRLF");
    }

    // F3: LineEnding::Lf leaves output unchanged (LF in, LF out).
    #[test]
    fn line_ending_lf_leaves_output_unchanged() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Lf,
            ..default_opts()
        };
        let output = format_yaml("a: 1\nb: 2\n", &opts);
        assert!(!output.contains('\r'), "LF mode should produce no CR");
        assert!(output.ends_with('\n'), "LF mode should end with LF");
    }

    // F4: insert_final_newline = false strips the trailing LF.
    #[test]
    fn insert_final_newline_false_strips_trailing_newline() {
        let opts = YamlFormatOptions {
            insert_final_newline: false,
            ..default_opts()
        };
        let output = format_yaml("key: value\n", &opts);
        assert_eq!(
            output, "key: value",
            "trailing newline should be stripped; got: {output:?}"
        );
    }

    // F5: insert_final_newline = true leaves the trailing LF in place.
    #[test]
    fn insert_final_newline_true_leaves_trailing_newline() {
        let opts = YamlFormatOptions {
            insert_final_newline: true,
            ..default_opts()
        };
        let output = format_yaml("key: value\n", &opts);
        assert!(
            output.ends_with('\n'),
            "trailing newline should be preserved; got: {output:?}"
        );
    }

    // F6: insert_final_newline = false with Crlf strips the trailing CRLF.
    #[test]
    fn insert_final_newline_false_with_crlf_strips_crlf_terminator() {
        let opts = YamlFormatOptions {
            line_ending: LineEnding::Crlf,
            insert_final_newline: false,
            ..default_opts()
        };
        let output = format_yaml("key: value\n", &opts);
        assert!(
            !output.ends_with("\r\n") && !output.ends_with('\n') && !output.ends_with('\r'),
            "trailing CRLF terminator should be stripped; got: {output:?}"
        );
        assert!(
            output.ends_with("value"),
            "content should be intact; got: {output:?}"
        );
    }

    // F7: Default options still end with newline (regression guard).
    #[test]
    fn format_yaml_default_options_still_ends_with_newline() {
        let output = format_yaml("key: value\n", &default_opts());
        assert!(
            output.ends_with('\n'),
            "default options should preserve trailing newline; got: {output:?}"
        );
    }

    // ---- Group: comments at every node position ----

    /// The shape of the data of `yaml`: structure, scalar values, aliases,
    /// anchors and tags; not spans, styles or comments.
    fn data_signature(yaml: &str) -> String {
        fn walk(node: &Node<Span>, out: &mut String) {
            let props = format!("&{:?} !{:?}", node.anchor(), node_tag(node));
            match node {
                Node::Scalar { value, .. } => {
                    let _ = write!(out, "S{props}{value:?};");
                }
                Node::Alias { name, .. } => {
                    let _ = write!(out, "A{name};");
                }
                Node::Mapping { entries, .. } => {
                    let _ = write!(out, "M{props}(");
                    for (key, value) in entries {
                        walk(key, out);
                        walk(value, out);
                    }
                    out.push_str(");");
                }
                Node::Sequence { items, .. } => {
                    let _ = write!(out, "Q{props}[");
                    for item in items {
                        walk(item, out);
                    }
                    out.push_str("];");
                }
            }
        }
        fn node_tag(node: &Node<Span>) -> Option<&str> {
            match node {
                Node::Scalar { tag, .. }
                | Node::Mapping { tag, .. }
                | Node::Sequence { tag, .. } => tag.as_deref(),
                Node::Alias { .. } => None,
            }
        }
        let mut out = String::new();
        for doc in rlsp_yaml_parser::load(yaml).expect("test input must parse") {
            walk(&doc.root, &mut out);
            out.push('\n');
        }
        out
    }

    /// The text of every comment of `yaml`, in source order.
    fn comment_texts(yaml: &str) -> Vec<String> {
        rlsp_yaml_parser::parse_events(yaml)
            .filter_map(|event| match event {
                Ok((Event::Comment { text }, _)) => Some(format!("#{text}")),
                _ => None,
            })
            .collect()
    }

    /// Formatting `input` gives output that loads to the same data, holds the
    /// same comments in the same order, and does not change when formatted again.
    fn assert_comments_kept(yaml: &str, opts: &YamlFormatOptions, label: &str) {
        let output = format_yaml(yaml, opts);
        let context = format!("{label}: input {yaml:?}, output {output:?}");
        assert!(
            rlsp_yaml_parser::load(&output).is_ok(),
            "output does not load: {context}"
        );
        assert_eq!(
            data_signature(&output),
            data_signature(yaml),
            "data differs: {context}"
        );
        let (before, after) = (comment_texts(yaml), comment_texts(&output));
        assert_eq!(
            after,
            before,
            "comments differ ({} in, {} out): {context}",
            before.len(),
            after.len()
        );
        assert_eq!(
            format_yaml(&output, opts),
            output,
            "output is not stable: {context}"
        );
    }

    /// Every position a comment can take at a node, for `assert_comments_kept`.
    const COMMENT_POSITIONS: &[&str] = &[
        // After a value, on the value's line.
        "a: v # c\n",
        "a: \"q\" # c\nb: 1\n",
        "a: 'q' # c\n",
        "x: &x 1\na: *x # c\n",
        "a: ~ # c\n",
        "a: {} # c\n",
        "a: [] # c\n",
        "a: [1, 2] # c\n",
        "a: {x: 1} # c\n",
        "- v # c\n",
        "- \"q\" # c\n",
        "- [1] # c\n",
        "a: v\t# c\n",
        // On a key's line.
        "k: # c\nb: 1\n",
        "k: # c\n",
        "k: # c\n  sub: 1\n",
        "k: # c\n  - 1\n",
        "k: # c\n- 1\n",
        "k: &x # c\n  - 1\n",
        "k: !t # c\n  a: 1\n",
        "k: &x # c\n  v\n",
        "k: # c\n  \"q\"\n",
        "k: # c\n  [1, 2]\n",
        "k: !!str # c\n",
        "k: &x # c\n",
        "*a : # c\n",
        "<<: # c\n  a: 1\n",
        "a: # c\n  b: # d\n    c: 1\n",
        // On a key's line and on its value's line.
        "k: # c\n  v # d\n",
        "k: # c\n  \"q\" # d\n",
        "k: # c\n  [1] # d\n",
        "k: &x # c\n  v # d\n",
        "# lead\nk: # c\n  v # d\n",
        "x:\n  # lead\n  k: # c\n    v # d\n",
        "a: 1\nb: # c\n  v # d\nc: 2\n",
        // In a sequence.
        "- # c\n  a: 1\n",
        "- # c\n  v\n",
        "- # c\n",
        "- # c\n- x\n",
        "- &x # c\n  a: 1\n",
        "- !t # c\n  a: 1\n",
        "- # c\n  - 1\n",
        "- - # c\n    a: 1\n",
        "- k: # c\n    v # d\n",
        "- k: # c\n    v\n",
        "- k: # c\n    sub: 1\n",
        "- a: 1\n  k: # c\n    v # d\n",
        "- - k: # c\n      v # d\n",
        "a:\n  - k: # c\n      v # d\n",
        "# lead\n- k: # c\n    v # d\n",
        "- - # c\n    x\n",
        // The dash line of a later item, and the `?` of a later key.
        "- a: 1\n- # d\n  b: 2\n",
        "- # c\n  a: 1\n- # d\n  b: 2\n",
        "- # c\n  k: # e\n    v # f\n- # g\n  l: 1\n",
        "a:\n  - b: 1\n  - # d\n    c: 2\nd: 3\n",
        "- - a: 1\n  - # d\n    b: 1\n",
        "a:\n  y: 1\n? # d\n  z\n: 1\n",
        "- # c\n- # d\n",
        "- !!str # c\n- # d\n- x\n",
        "- &x # c\n- # d\n",
        "- !!str\n# d\n- x\n",
        "a: &x\n# d\nb: 1\n",
        "a:\n- # c\n- # d\n",
        // A multi-line quoted scalar followed by a comment.
        "a: \"x\n  y\" # c\nb: 1\n",
        "- 'x\n  y' # c\n- b\n",
        // Explicit keys.
        "? k # c\n: v\n",
        "? k # c\n: v # d\n",
        "? k\n: v # c\n",
        "? # c\n  k\n: v\n",
        "? # c\n: v\n",
        "? k # c\n:\n",
        "? k\n: # c\n",
        "? k\n: # c\n  v\n",
        "? k\n# c\n: v\n",
        "? [a, b] # c\n: v # d\n",
        "? - a # c\n  - b\n: v\n",
        "? a: 1 # c\n  b: 2\n: v\n",
        "? a: # c\n    v # d\n  b: 1\n: x\n",
        "? | # c\n  k\n: | # d\n  v\n",
        "? > # c\n  k\n: v # d\n",
        "? k\n: # c\n  - 1\n",
        "? k\n: # c\n  a: 1\n",
        "- ? k # c\n  : v\n",
        // The empty key.
        ": v # c\n",
        ": # c\n  v\n",
        ": | # c\n  t\n",
        ": # c\n  a: 1\n",
        ": # c\n  - 1\n",
        "? # c\n: # d\n  a: 1\n",
        ": &x # c\n  a: 1\n",
        // Block scalar headers.
        "a: | # c\n  t\n",
        "a: |- # c\n  t\n",
        "a: |+ # c\n  t\n\nb: 1\n",
        "a: > # c\n  t\n",
        "a: >- # c\n  t\n",
        "a: |2 # c\n   t\n",
        "a: | # c\n  # d\n  t\n",
        "a: | # |- x: y\n  t\n",
        "a: | # c\nb: 1\n",
        "a: &x | # c\n  t\n",
        "a: !!str | # c\n  t\n",
        "a: |+ # c\n  x\n   \nb: 1\n",
        "- | # c\n  t\n- x\n",
        "- |+ # c\n  x\n   \n",
        "- k: | # c\n    t\n",
        "k: # c\n  | # d\n  t\n",
        "k: # c\n  |\n  t\n",
        "a: | # c\n  t\n---\nb: | # d\n  u\n",
        "--- | # c\n  x\n",
        "--- >- # c\n  x\n",
        "| # c\n  x\n",
        // The root and several documents.
        "--- \"q\" # c\n",
        "--- &x # c\na: 1\n",
        "--- !t # c\na: 1\n",
        "--- &x # c\n- 1\n",
        "--- v # c\n--- w # d\n",
        // Comments between entries and around block scalars.
        "a: |+ # c\n  x\n# d\nb: 1\n",
        "a:\n  b: |\n    x\n  # d\nz: 1\n",
        // Several lines of a plain or quoted scalar.
        "a:\n  v\n  w # c\n",
        "a: v\n\n  w # c\n",
        // Comments inside flow collections.
        "a: [\n  # c\n  1\n  ]\n",
        "a: {\n  # c\n  b: 1\n  }\n",
        "[ # c\n 1]\n",
        "[1,\n # c\n 2]\n",
        "a: [1, # c\n 2]\n",
        "a: [1 # c\n , 2]\n",
        "a: {b: # c\n 1}\n",
        "{ # c\n b: 1}\n",
        "{b: 1, # c\n d: 2}\n",
        "[\n # a\n # b\n 1, # c\n 2 # d\n]\n",
        "- [ # c\n  1]\n- x\n",
        "- k: [ # c\n    1]\n",
        "- { # c\n  k: 1}\n- x\n",
        "{a: [ # c\n 1]}\n",
        "? [ # c\n  a, b]\n: v\n",
        "? { # c\n  a: 1}\n: v\n",
        // A comment on a flow collection itself, or around it, keeps it flow.
        "# c\na: [1]\n",
        "# c\n[1, 2]\n",
        "[1, 2] # c\n",
        "- [1] # c\n- [2]\n",
        "x: 1 # c\ny: [1, 2]\n",
        // A comment before the closing bracket is not a comment of an item.
        "a: [1,\n # c\n ]\nb: 2\n",
        "a: {b\n # c\n : 1}\n",
        // Nested flow collections.
        "a: [x, [ # c\n y], z]\n",
        "[ # c\n [1, 2], {a: 1}]\n",
        "[[ # c\n 1], [2]]\n",
        "[[1] # c\n, 2]\n",
        "{a: [1] # c\n, b: 2}\n",
        "[[[ # c\n 1]]]\n",
        "{a: [ {b: 1, # c\n d: 2} ], e: 3}\n",
        // Properties, aliases, and flow-only syntax.
        "a: &x [ # c\n 1]\n",
        "a: !t [ # c\n 1]\n",
        "a: &x !t { # c\n b: 1}\n",
        "x: &y 1\na: [ # c\n *y, &z 2, *z]\n",
        "{ # c\n a, b: , c: 1}\n",
        "[ # c\n a: 1, b]\n",
        "x: &a k\ny: { # c\n *a : 1}\n",
        // Scalars that are quoted or plain for the flow context.
        "[ # c\n \"a,b\", \"[x]\", \"{y}\", \"a: b\", \"- z\", \"#x\"]\n",
        "[ # c\n a:b, ?x, -y, \":\", \"\", \"null\", \"123\"]\n",
        "[ # c\n \"x\n  y\", a\n b]\n",
        "[ # c\n [aaaa, bbbb, cccc, dddd], 1]\n",
        "a: [1, 2]\nb: [ # c\n 3]\n",
    ];

    /// The option sets a comment must survive.  Each is applied to the input as
    /// written, except the line-ending set, whose input uses CRLF.
    fn option_sets() -> Vec<(&'static str, YamlFormatOptions)> {
        let base = default_opts;
        vec![
            ("default", base()),
            (
                "enforce block style",
                YamlFormatOptions {
                    format_enforce_block_style: true,
                    ..base()
                },
            ),
            (
                "narrow",
                YamlFormatOptions {
                    print_width: 20,
                    ..base()
                },
            ),
            (
                "indentless sequences",
                YamlFormatOptions {
                    format_indent_sequences: false,
                    ..base()
                },
            ),
            (
                "preserve quotes",
                YamlFormatOptions {
                    preserve_quotes: true,
                    ..base()
                },
            ),
            (
                "single quote",
                YamlFormatOptions {
                    single_quote: true,
                    ..base()
                },
            ),
            (
                "yaml 1.1",
                YamlFormatOptions {
                    yaml_version: YamlVersion::V1_1,
                    ..base()
                },
            ),
            (
                "no final newline",
                YamlFormatOptions {
                    insert_final_newline: false,
                    ..base()
                },
            ),
            (
                "remove duplicate keys",
                YamlFormatOptions {
                    format_remove_duplicate_keys: true,
                    ..base()
                },
            ),
            (
                "crlf",
                YamlFormatOptions {
                    line_ending: LineEnding::Crlf,
                    ..base()
                },
            ),
        ]
    }

    // Scalars whose quoting differs between the flow and the block context, in a
    // flow collection that changes style because of a comment.
    #[rstest]
    #[case::double_quoted_with_indicators(
        "[ # c\n \"&x\", \"*x\", \"!t\", \"@x\", \"`x\", \"%x\", \"|\", \">\", \"'\", \"? x\", \": x\", \"x #y\", \"x: y\", \"- x\", \" lead\", \"trail \"]\n"
    )]
    #[case::single_quoted_with_indicators(
        "[ # c\n '&x', '*x', '!t', '@x', '%x', '|', '>', '\"', '? x', ': x', 'x #y', 'x: y', '- x', ' lead', 'trail ', 'a,b', '[x]', '{y}']\n"
    )]
    #[case::words_the_core_schema_reads_as_other_types(
        "[ # c\n \"null\", \"true\", \"1e3\", \"~\", \"\", null, true, 1e3, ~, .inf]\n"
    )]
    #[case::plain_scalars_with_flow_safe_indicators("[ # c\n a:b, ?x, -y, a#b, x-y, :x]\n")]
    #[case::scalars_that_fold_over_lines("[ # c\n \"x\n  y\", 'p\n  q', a\n b]\n")]
    #[case::scalars_in_a_flow_mapping(
        "{ # c\n \"a,b\": \"[x]\", 'k: l': '{y}', \"#k\": \"v #w\", plain: text}\n"
    )]
    #[case::null_and_empty_entries("{ # c\n a, b: , c: 1}\n")]
    #[case::empty_key("{ # c\n : x, y: 1}\n")]
    #[case::single_pair_mapping_in_a_sequence("[ # c\n a: 1, b, : d, [e]: f]\n")]
    #[case::tagged_and_anchored_items("[ # c\n !!null \"\", !t x, &a !t y, &b z, *b]\n")]
    #[case::tagged_and_anchored_keys_and_values(
        "{ # c\n !!null : v, &a k: &b v, !t x: w, ? !!str z: x, *a : *b}\n"
    )]
    #[case::alias_keys_and_values("x: &a k\ny: &b v\nz: { # c\n *a : *b, k: *b}\n")]
    #[case::flow_collections_as_keys("{ # c\n [a, b]: 1, [ # d\n e]: 3}\n")]
    #[case::empty_double_quoted_key("{ # c\n \"\": 1}\n")]
    #[case::empty_single_quoted_key("{ # c\n '': 1}\n")]
    #[case::anchored_empty_string_key("{ # c\n &a \"\": x, y: *a}\n")]
    #[case::anchored_empty_plain_key("{ # c\n &a : x, y: *a}\n")]
    #[case::anchored_flow_mapping_with_an_empty_collection_as_first_key(
        "&z { # c\n {}: 1, [] : 2}\n"
    )]
    #[case::anchored_flow_mapping_with_an_empty_sequence_as_first_key("&z { # c\n []: 1}\n")]
    #[case::tagged_flow_mapping_with_a_tagged_empty_collection_as_first_key(
        "!u { # c\n !u [] : 1}\n"
    )]
    #[case::anchored_flow_mapping_value_with_an_empty_collection_as_first_key(
        "k: &z { # c\n {}: 1}\n"
    )]
    #[case::anchored_flow_mapping_item_with_an_empty_collection_as_first_key(
        "- &z { # c\n {}: 1}\n"
    )]
    #[case::anchored_flow_sequence_at_the_root("&z [ # c\n a]\n")]
    #[case::tagged_flow_mapping_at_the_root("!t { # c\n a: 1}\n")]
    #[case::anchored_and_tagged_flow_sequence_at_the_root("&z !t [ # c\n a]\n")]
    #[case::anchored_flow_sequence_as_item("- &z [ # c\n a]\n")]
    #[case::tagged_empty_key_after_an_empty_value("{ a, !t : 1, # c\n}\n")]
    #[case::tagged_empty_core_key_after_an_empty_value("{ # c\n a: , !!null : 1}\n")]
    #[case::empty_key_after_an_empty_value("{ # c\n a, : x, c: 1}\n")]
    #[case::anchored_empty_key_after_an_empty_value("{ # c\n a, &k : 1, c: *k}\n")]
    #[case::empty_key_with_an_empty_value_after_an_empty_value("{ # c\n a, : , b: 1}\n")]
    #[case::alias_key_after_an_empty_value("x: &p k\ny: { # c\n a, *p : 1, b}\n")]
    #[case::alias_key_with_an_anchored_block_value("x: &a k\ny: { # c\n *a : &y [ 1 ]}\n")]
    #[case::tagged_empty_key_with_an_anchored_block_value("{ # c\n !t : &y [ 1 ]}\n")]
    #[case::tagged_empty_key_with_an_anchored_and_tagged_mapping_value(
        "{ # c\n !t : &y !u { b: 1 }}\n"
    )]
    #[case::empty_collections_beside_the_comment("[ # c\n [], {}, [[]], {a: []}]\n")]
    fn flow_collection_with_comment_keeps_its_data(#[case] input: &str) {
        for (name, opts) in option_sets() {
            let input = if opts.line_ending == LineEnding::Crlf {
                input.replace('\n', "\r\n")
            } else {
                input.to_string()
            };
            assert_comments_kept(&input, &opts, name);
        }
    }

    // An implicit key of the longest length the parser accepts stays one in the
    // block style the collection changes to.
    #[test]
    fn flow_mapping_with_comment_keeps_a_key_of_the_longest_accepted_length() {
        let key = "k".repeat(1024);
        assert_comments_kept(
            &format!("{{ # c\n {key}: 1}}\n"),
            &default_opts(),
            "default",
        );
    }

    // A mapping key after the first entry has no block form the parser reads
    // (`x: 1`, then `? a: 1`, then `: v`), and neither has a key that holds an
    // entry written with `? ` (`? ? a`), so a flow collection key stays flow and
    // its comments are printed above the entry.  The enforce-block-style option
    // prints every collection in block style, these keys too, and is left out
    // here.
    #[rstest]
    #[case::mapping_key_after_the_first_entry("{ x: 1, { # c\n a: 1}: v }\n")]
    #[case::mapping_key_with_nested_comments("{ x: 1, { # c\n a: 1, b: [ # d\n 2]}: v, y: 2 }\n")]
    #[case::mapping_key_in_a_sequence_item("- { x: 1, { # c\n a: 1}: v }\n- { y: 2 }\n")]
    #[case::mapping_key_in_a_mapping_value("k: { x: 1, { # c\n a: 1}: v }\n")]
    #[case::mapping_key_with_a_comment_after_the_bracket("{ x: 1, { # c\n a: 1} # d\n : v }\n")]
    #[case::mapping_key_as_the_first_entry("{ { # c\n a: 1}: v, x: 1 }\n")]
    #[case::mapping_key_with_properties_and_an_empty_collection_as_first_key(
        "? &z { # c\n {}: 1}\n: v\n"
    )]
    #[case::mapping_key_whose_first_key_is_a_collection("{ { # c\n [a]: 1}: v, x: 1 }\n")]
    #[case::sequence_key_after_the_first_entry("{ x: 1, [ # c\n a]: v }\n")]
    #[case::sequence_key_holding_a_mapping_with_a_collection_key("{ [ { [ a ]: #c\n b } ]: v }\n")]
    #[case::flow_source_sequence_key_holding_a_mapping_with_a_collection_key(
        "? [ { [ a ]: #c\n b } ]\n: v\n"
    )]
    #[case::sequence_key_with_properties_holding_collection_keys(
        "&z { # c\n &k [ {}, { {a: 1}: !u 2 } ]: v }\n"
    )]
    #[case::sequence_key_in_a_mapping_value("k: { [ { [ a ]: #c\n b } ]: v }\n")]
    #[case::sequence_key_in_a_sequence_item("- { [ { [ a ]: #c\n b } ]: v }\n")]
    #[case::mapping_key_in_a_sequence_key("{ x: 1, [ { # c\n a: 1} ]: v }\n")]
    fn flow_collection_key_keeps_its_comments_above_the_entry(#[case] input: &str) {
        for (name, opts) in option_sets() {
            let input = if opts.line_ending == LineEnding::Crlf {
                input.replace('\n', "\r\n")
            } else {
                input.to_string()
            };
            assert_comments_kept(&input, &opts, name);
        }
    }

    // The comment after the closing bracket of a collection that changes style
    // because of another comment has no line of its own in block style: it moves
    // to the line of the key, or above the first entry when the key is written
    // with `? `, where a comment after the colon reads as one of the first entry.
    // It comes before the comments inside the collection, which differs from the
    // order of the input, so only the comments, the data and the stability of
    // the output are compared.
    #[rstest]
    #[case::sequence_value_of_an_explicit_key(
        "{ [ a ]: [ x #c\n ] #d\n }\n",
        "? [a]\n:\n  #d\n  - x  #c\n"
    )]
    #[case::mapping_value_of_an_explicit_key(
        "{ [ a ]: { # c\n b: 1 } #d\n }\n",
        "? [a]\n:\n  #d\n  # c\n  b: 1\n"
    )]
    #[case::sequence_value_of_an_implicit_key("k: [ x #c\n ] #d\n", "k:  #d\n  - x  #c\n")]
    fn comment_after_the_bracket_of_a_flipped_value_moves_before_the_comments_inside(
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        assert_eq!(data_signature(&output), data_signature(input));
        assert_eq!(format_yaml(&output, &default_opts()), output);
        let sorted = |yaml: &str| {
            let mut comments = comment_texts(yaml);
            comments.sort();
            comments
        };
        assert_eq!(sorted(&output), sorted(input));
    }

    // The deepest nesting the parser accepts: every level changes style because
    // of one comment at the bottom.  The loader and the renderers recurse once
    // per level, and a debug build overflows the 2 MiB stack of a test thread
    // in the loader at about 225 levels, before it reaches the formatter, so the
    // test runs on a larger stack; a release build runs this depth on 2 MiB.
    #[rstest]
    #[case::sequences("[", "]")]
    #[case::mappings("{a: ", "}")]
    fn flow_collections_nested_as_deep_as_the_parser_accepts_keep_a_comment(
        #[case] open: &'static str,
        #[case] close: &'static str,
    ) {
        std::thread::Builder::new()
            .stack_size(64 << 20)
            .spawn(move || {
                let levels = rlsp_yaml_parser::MAX_COLLECTION_DEPTH - 1;
                let input = format!("{} # c\n 1{}\n", open.repeat(levels), close.repeat(levels));
                let output = format_yaml(&input, &default_opts());
                assert!(!output.contains(['[', '{']), "style did not change");
                assert_comments_kept(&input, &default_opts(), "default");
            })
            .expect("the thread starts")
            .join()
            .expect("the formatter handles the deepest nesting the parser accepts");
    }

    #[test]
    fn every_comment_position_is_kept_under_every_option_set() {
        for (name, opts) in option_sets() {
            for input in COMMENT_POSITIONS {
                let input = if opts.line_ending == LineEnding::Crlf {
                    input.replace('\n', "\r\n")
                } else {
                    (*input).to_string()
                };
                assert_comments_kept(&input, &opts, name);
            }
        }
    }

    #[rstest]
    #[case::leading_comment_stays_above_hoisted_key_comment(
        "x:\n  # lead\n  k: # c\n    v # d\n",
        "x:\n  # lead\n  # c\n  k: v  # d\n"
    )]
    #[case::hoisted_key_comment_in_second_entry_of_item(
        "- a: 1\n  k: # c\n    v # d\n",
        "- a: 1\n  # c\n  k: v  # d\n"
    )]
    #[case::hoisted_key_comment_above_first_entry_of_item_stays_above_the_dash(
        "a:\n  - k: # c\n      v # d\n",
        "a:\n  # c\n  - k: v  # d\n"
    )]
    #[case::hoisted_key_comment_follows_leading_comment_of_item(
        "# lead\n- k: # c\n    v # d\n",
        "# lead\n# c\n- k: v  # d\n"
    )]
    #[case::hoisted_key_comment_of_item_in_item(
        "- - k: # c\n      v # d\n",
        "# c\n- \n  - k: v  # d\n"
    )]
    #[case::hoisted_key_comment_of_explicit_mapping_key_goes_above_the_question_mark(
        "? a: # c\n    v # d\n  b: 1\n: x\n",
        "# c\n? a: v  # d\n  b: 1\n: x\n"
    )]
    #[case::key_comment_of_item_entry_joins_the_value("- k: # c\n    v\n", "- k: v  # c\n")]
    #[case::alias_key_without_value_keeps_the_space_before_the_colon(
        "&a x: 1\n*a : # c\n",
        "&a x: 1\n*a :  # c\n"
    )]
    #[case::explicit_value_block_sequence_follows_the_entry(
        "? k\n: # c\n  - 1\n",
        "k:\n  # c\n  - 1\n"
    )]
    #[case::explicit_block_sequence_key_keeps_the_comment_of_its_item(
        "? - a # c\n  - b\n: v\n",
        "? - a  # c\n  - b\n: v\n"
    )]
    #[case::anchor_before_block_scalar("a: &x | # c\n  t\n", "a: &x |  # c\n  t\n")]
    #[case::empty_block_scalar_keeps_its_header_comment("a: |- # c\nb: 1\n", "a: |-  # c\nb: 1\n")]
    #[case::block_scalar_content_comment_line_is_content(
        "a: | # c\n  # d\n  t\n",
        "a: |  # c\n  # d\n  t\n"
    )]
    #[case::keep_chomp_header_comment_with_blank_lines(
        "a: |+ # c\n  t\n\nb: 1\n",
        "a: |+  # c\n  t\n\nb: 1\n"
    )]
    #[case::comment_after_keep_scalar_stays_below_it(
        "a: |+ # c\n  x\n# d\nb: 1\n",
        "a: |+  # c\n  x\n# d\nb: 1\n"
    )]
    #[case::header_comment_with_block_scalar_indicators(
        "a: | # |- x: y\n  t\n",
        "a: |  # |- x: y\n  t\n"
    )]
    #[case::double_quoted_comment_with_hash_and_quote(
        "a: \"q # x\" # it's\n",
        "a: \"q # x\"  # it's\n"
    )]
    #[case::block_scalar_with_space_only_line_is_quoted_and_comment_follows(
        "a: |+ # c\n  x\n   \nb: 1\n",
        "a: \"x\\n \\n\"  # c\nb: 1\n"
    )]
    #[case::block_scalar_item_with_space_only_line_is_quoted_and_comment_follows(
        "- |+ # c\n  x\n   \n",
        "- \"x\\n \\n\"  # c\n"
    )]
    #[case::null_item_with_comment_has_the_comment_above("- # c\n- x\n", "# c\n- \n- x\n")]
    #[case::scalar_item_with_comment_on_dash_line_has_the_comment_above(
        "- # c\n  v\n",
        "# c\n- v\n"
    )]
    #[case::empty_key_block_mapping_value_keeps_the_key_comment(
        ": # c\n  a: 1\n",
        ":  # c\n  a: 1\n"
    )]
    #[case::empty_key_block_sequence_value_keeps_the_key_comment(
        ": # c\n  - 1\n",
        ":  # c\n  - 1\n"
    )]
    #[case::empty_key_with_both_comments_and_block_value(
        "? # c\n: # d\n  a: 1\n",
        "# c\n:  # d\n  a: 1\n"
    )]
    #[case::empty_key_anchored_block_value(": &x # c\n  a: 1\n", ": &x  # c\n  a: 1\n")]
    #[case::empty_key_block_value_without_comment(":\n  a: 1\n", ":\n  a: 1\n")]
    #[case::dash_of_second_item_after_mapping_item(
        "- a: 1\n- # d\n  b: 2\n",
        "- a: 1\n# d\n- b: 2\n"
    )]
    #[case::dash_of_both_items("- # c\n  a: 1\n- # d\n  b: 2\n", "# c\n- a: 1\n# d\n- b: 2\n")]
    #[case::dash_of_items_with_key_and_value_comments(
        "- # c\n  k: # e\n    v # f\n- # g\n  l: 1\n",
        "# c\n# e\n- k: v  # f\n# g\n- l: 1\n"
    )]
    #[case::question_mark_after_nested_mapping(
        "a:\n  y: 1\n? # d\n  z\n: 1\n",
        "a:\n  y: 1\n# d\nz: 1\n"
    )]
    #[case::dash_of_second_item_in_nested_sequence(
        "a:\n  - b: 1\n  - # d\n    c: 2\nd: 3\n",
        "a:\n  - b: 1\n  # d\n  - c: 2\nd: 3\n"
    )]
    #[case::dash_of_second_inner_item(
        "- - a: 1\n  - # d\n    b: 1\n",
        "- \n  - a: 1\n  # d\n  - b: 1\n"
    )]
    #[case::two_empty_items_with_dash_comments("- # c\n- # d\n", "# c\n- \n# d\n- \n")]
    #[case::multi_line_double_quoted_value_keeps_its_comment(
        "a: \"x\n  y\" # c\nb: 1\n",
        "a: x y  # c\nb: 1\n"
    )]
    #[case::properties_line_comment_and_dash_comment_of_next_item(
        "- !!str # c\n- # d\n- x\n",
        "# c\n- !!str\n# d\n- \n- x\n"
    )]
    #[case::anchor_only_item_and_dash_comment_of_next_item(
        "- &x # c\n- # d\n",
        "# c\n- &x\n# d\n- \n"
    )]
    #[case::comment_below_tag_only_item_stays_below("- !!str\n# d\n- x\n", "- !!str\n# d\n- x\n")]
    #[case::comment_below_tag_only_item_after_blank_line(
        "- !!str\n\n# d\n\n- x\n",
        "- !!str\n\n# d\n\n- x\n"
    )]
    #[case::comment_below_anchor_only_value("a: &x\n# d\nb: 1\n", "a: &x\n# d\nb: 1\n")]
    #[case::tab_before_comment_becomes_two_spaces("a: v\t# c\n", "a: v  # c\n")]
    #[case::hash_without_space_is_no_comment("a: v#c\n", "a: v#c\n")]
    #[case::comment_with_nel_stays_one_comment("a: v # x\u{85}y\n", "a: v  # x\u{85}y\n")]
    #[case::key_comment_hoisted_with_nel("k: # x\u{85}y\n  v # d\n", "# x\u{85}y\nk: v  # d\n")]
    #[case::comment_with_trailing_spaces_keeps_them("a: v # c  \n", "a: v  # c  \n")]
    #[case::empty_comment("a: v #\n", "a: v  #\n")]
    #[case::hoisted_empty_comment("k: #\n  v # d\n", "#\nk: v  # d\n")]
    #[case::flow_sequence_with_comment_becomes_block(
        "a: [\n  # c\n  1\n  ]\n",
        "a:\n  # c\n  - 1\n"
    )]
    #[case::flow_mapping_with_comment_becomes_block(
        "a: {\n  # c\n  b: 1\n  }\n",
        "a:\n  # c\n  b: 1\n"
    )]
    #[case::root_flow_sequence_comment_after_bracket("[ # c\n 1]\n", "# c\n- 1\n")]
    #[case::flow_sequence_comment_before_second_item("[1,\n # c\n 2]\n", "- 1\n# c\n- 2\n")]
    #[case::flow_sequence_trailing_comment_of_item("a: [1, # c\n 2]\n", "a:\n  - 1  # c\n  - 2\n")]
    #[case::flow_mapping_comment_on_key_joins_the_value("a: {b: # c\n 1}\n", "a:\n  b: 1  # c\n")]
    #[case::root_flow_mapping_comment_before_key("{ # c\n b: 1}\n", "# c\nb: 1\n")]
    #[case::flow_mapping_trailing_comment_of_value("{b: 1, # c\n d: 2}\n", "b: 1  # c\nd: 2\n")]
    #[case::flow_sequence_item_with_comment_becomes_block(
        "- [ # c\n  1]\n- x\n",
        "# c\n- \n  - 1\n- x\n"
    )]
    #[case::flow_comment_on_the_collection_keeps_it_flow("a: [1] # c\n", "a: [1]  # c\n")]
    #[case::flow_mapping_comment_on_the_collection_keeps_it_flow(
        "a: {b: 1} # c\n",
        "a: { b: 1 }  # c\n"
    )]
    #[case::flow_comment_above_the_entry_keeps_it_flow("# c\na: [1]\n", "# c\na: [1]\n")]
    #[case::comment_before_the_closing_bracket_goes_above_the_next_entry(
        "a: [1,\n # c\n ]\nb: 2\n",
        "a: [1]\n# c\nb: 2\n"
    )]
    #[case::inner_flow_comment_makes_every_level_block(
        "a: [x, [ # c\n y], z]\n",
        "a:\n  - x\n  # c\n  - \n    - y\n  - z\n"
    )]
    #[case::sibling_flow_collections_without_comment_stay_flow(
        "[ # c\n [1, 2], {a: 1}]\n",
        "# c\n- [1, 2]\n- { a: 1 }\n"
    )]
    #[case::only_the_collection_with_a_comment_changes_style(
        "[[ # c\n 1], [2]]\n",
        "# c\n- \n  - 1\n- [2]\n"
    )]
    #[case::comment_on_inner_collection_makes_the_outer_block(
        "[[1] # c\n, 2]\n",
        "- [1]  # c\n- 2\n"
    )]
    #[case::sibling_without_comment_stays_flow(
        "a: [1, 2]\nb: [ # c\n 3]\n",
        "a: [1, 2]\nb:\n  # c\n  - 3\n"
    )]
    #[case::anchor_of_flow_collection_with_comment_stays_before_the_items(
        "a: &x [ # c\n 1]\n",
        "a: &x\n  # c\n  - 1\n"
    )]
    #[case::quoted_empty_key_is_not_the_empty_key("\"\": 1\n", "\"\": 1\n")]
    #[case::single_quoted_empty_key_is_not_the_empty_key("'': 1\n", "'': 1\n")]
    #[case::anchored_quoted_empty_key_keeps_its_anchor(
        "&a \"\": x\nb: *a\n",
        "&a \"\": x\nb: *a\n"
    )]
    #[case::anchored_plain_empty_key_keeps_its_anchor("&a : x\nb: *a\n", "&a : x\nb: *a\n")]
    #[case::empty_string_key_of_flow_mapping_with_comment("{ # c\n \"\": 1}\n", "# c\n\"\": 1\n")]
    #[case::empty_collection_as_first_key_below_properties_is_explicit(
        "&z\n? []\n: 1\n",
        "&z\n? []\n: 1\n"
    )]
    #[case::tagged_empty_collection_as_first_key_below_properties_is_explicit(
        "&z\n? !u []\n: 1\n",
        "&z\n? !u []\n: 1\n"
    )]
    #[case::empty_collection_as_later_key_below_properties_stays_implicit(
        "&z\na: 1\n[]: 2\n",
        "&z\na: 1\n[]: 2\n"
    )]
    #[case::flipped_root_mapping_with_properties_and_empty_collection_key(
        "&z { # c\n {}: 1}\n",
        "# c\n&z\n? {}\n: 1\n"
    )]
    #[case::flipped_mapping_value_with_properties_and_empty_collection_key(
        "k: &z { # c\n {}: 1}\n",
        "k: &z\n  # c\n  ? {}\n  : 1\n"
    )]
    #[case::comment_of_flipped_root_with_properties_goes_above_them(
        "&z [ # c\n a]\n",
        "# c\n&z\n- a\n"
    )]
    #[case::comment_of_flipped_root_mapping_with_a_tag_goes_above_it(
        "!t { # c\n a: 1}\n",
        "# c\n!t\na: 1\n"
    )]
    #[case::mapping_key_after_the_first_entry_stays_flow_with_comments_above_the_entry(
        "{ x: 1, { # c\n a: 1}: v }\n",
        "x: 1\n# c\n? { a: 1 }\n: v\n"
    )]
    #[case::mapping_key_after_the_first_entry_collects_every_inner_comment_in_order(
        "{ x: 1, { # c\n a: 1, # d\n b: [ # e\n 2]}: v, y: 2 }\n",
        "x: 1\n# c\n# d\n# e\n? { a: 1, b: [2] }\n: v\ny: 2\n"
    )]
    #[case::mapping_key_as_the_first_entry_stays_flow(
        "{ { # c\n a: 1}: v, x: 1 }\n",
        "# c\n? { a: 1 }\n: v\nx: 1\n"
    )]
    #[case::block_mapping_key_keeps_its_comments_where_they_are(
        "? a: 1 # c\n  b: 2\n: v\n",
        "? a: 1  # c\n  b: 2\n: v\n"
    )]
    #[case::alias_key_with_an_anchored_block_value_keeps_the_space_before_the_colon(
        "x: &a k\n*a : &y\n  k: 1\n",
        "x: &a k\n*a : &y\n  k: 1\n"
    )]
    #[case::tagged_empty_key_with_an_anchored_block_value_keeps_the_space_before_the_colon(
        "!t : &y\n  - 1\n",
        "!t : &y\n  - 1\n"
    )]
    #[case::tagged_empty_key_after_an_empty_value_is_implicit(
        "{ a, !t : 1, # c\n}\n",
        "a: \n!t : 1  # c\n"
    )]
    #[case::empty_key_after_an_empty_value_is_implicit(
        "{ # c\n a, : x, c: 1}\n",
        "# c\na: \n: x\nc: 1\n"
    )]
    #[case::explicit_tagged_empty_key_after_an_empty_value_prints_implicit(
        "a:\n? !t\n: 1\nb: 2\n",
        "a: \n!t : 1\nb: 2\n"
    )]
    #[case::explicit_empty_key_after_an_empty_value_prints_implicit("a:\n?\n: x\n", "a: \n: x\n")]
    #[case::tagged_empty_key_after_a_value_stays_implicit("a: 1\n!t : 2\n", "a: 1\n!t : 2\n")]
    #[case::sequence_key_with_a_comment_stays_flow_with_the_comment_above_the_entry(
        "{ [ # c\n a ]: v }\n",
        "# c\n? [a]\n: v\n"
    )]
    fn comment_position_prints_exactly(#[case] input: &str, #[case] expected: &str) {
        assert_eq!(format_yaml(input, &default_opts()), expected);
        assert_comments_kept(input, &default_opts(), "default");
    }

    #[rstest]
    #[case::block_sequence_value_stays_indentless(
        YamlFormatOptions { format_indent_sequences: false, ..default_opts() },
        "k: # c\n- 1\n",
        "k:  # c\n- 1\n"
    )]
    #[case::enforce_block_keeps_empty_flow_collection_inline(
        YamlFormatOptions { format_enforce_block_style: true, ..default_opts() },
        "a: {} # c\n",
        "a: {}  # c\n"
    )]
    #[case::enforce_block_value_with_key_comment(
        YamlFormatOptions { format_enforce_block_style: true, ..default_opts() },
        "a: # c\n  [1, 2] # d\n",
        "# c\na:  # d\n  - 1\n  - 2\n"
    )]
    #[case::wrapped_flow_value_keeps_comment_after_the_last_line(
        YamlFormatOptions { print_width: 20, ..default_opts() },
        "k: [alpha, bravo, charlie] # c\n",
        "k: [\n  alpha,\n  bravo,\n  charlie\n  ]  # c\n"
    )]
    #[case::enforce_block_flow_collection_with_comment_prints_like_the_default(
        YamlFormatOptions { format_enforce_block_style: true, ..default_opts() },
        "a: [ # c\n 1]\nb: [2]\n",
        "a:\n  # c\n  - 1\nb:\n  - 2\n"
    )]
    #[case::inner_flow_collection_without_comment_wraps_inside_the_block_parent(
        YamlFormatOptions { print_width: 10, ..default_opts() },
        "[ # c\n [aaaa, bbbb, cccc, dddd], 1]\n",
        "# c\n- [\n  aaaa,\n  bbbb,\n  cccc,\n  dddd\n  ]\n- 1\n"
    )]
    #[case::crlf_header_comment(
        YamlFormatOptions { line_ending: LineEnding::Crlf, ..default_opts() },
        "a: | # c\r\n  t\r\n",
        "a: |  # c\r\n  t\r\n"
    )]
    #[case::no_final_newline_after_header_comment(
        YamlFormatOptions { insert_final_newline: false, ..default_opts() },
        "a: | # c\n  t\n",
        "a: |  # c\n  t"
    )]
    fn comment_position_prints_exactly_with_options(
        #[case] opts: YamlFormatOptions,
        #[case] input: &str,
        #[case] expected: &str,
    ) {
        assert_eq!(format_yaml(input, &opts), expected);
        assert_comments_kept(input, &opts, "options");
    }

    #[test]
    fn duplicate_key_removal_keeps_comments_of_surviving_entries() {
        let opts = YamlFormatOptions {
            format_remove_duplicate_keys: true,
            ..default_opts()
        };
        let input = "a: 1\nb: # c\n  v # d\na: 2\n";
        let output = format_yaml(input, &opts);
        assert_eq!(output, "# c\nb: v  # d\na: 2\n");
        assert_eq!(format_yaml(&output, &opts), output);
    }

    // A comment that moves above its entry takes the blank lines of the input
    // with it only when the lines around it still pair up; the lines of the
    // block scalar and the comment itself never get a blank line inside.
    #[rstest]
    #[case::header_comment_between_blank_lines(
        "a: 1\n\nb: | # c\n  t\n\nc: 1\n",
        "a: 1\n\nb: |  # c\n  t\n\nc: 1\n"
    )]
    #[case::slot_comment_keeps_blank_lines(
        "a: 1\n\nk:  # c\n  sub: 1\n\nb: 1\n",
        "a: 1\n\nk:  # c\n  sub: 1\n\nb: 1\n"
    )]
    #[case::moved_comment_stays_glued_to_its_entry_and_blank_lines_are_lost(
        "a: 1\n\nk: # c\n  v # d\n\nb: 1\n",
        "a: 1\n# c\nk: v  # d\nb: 1\n"
    )]
    #[case::leading_comment_line_stays_glued_to_following_entry_and_blank_lines_are_lost(
        "k: # c\n  v # d\n\n# e\nb: 1\n",
        "# c\nk: v  # d\n# e\nb: 1\n"
    )]
    fn moved_comment_does_not_disturb_blank_lines(#[case] input: &str, #[case] expected: &str) {
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, expected);
        assert_eq!(scalar_values(&output), scalar_values(input));
        assert_comments_kept(input, &default_opts(), "default");
    }

    // Moving a comment to its own line can make it a line the server reads: a
    // suppression comment that was inactive at the end of a key's line becomes
    // active above the entry.  Pinned so the behavior change stays visible.
    #[test]
    fn key_comment_that_moves_above_its_entry_becomes_a_suppression_comment() {
        let input = "k: # rlsp-yaml-disable-next-line\n  v # d\n";
        assert!(
            !crate::validation::suppression::build_suppression_map(input).is_suppressed(1, "any")
        );
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, "# rlsp-yaml-disable-next-line\nk: v  # d\n");
        let suppression = crate::validation::suppression::build_suppression_map(&output);
        assert!(suppression.is_suppressed(1, "any"));
        assert!(!suppression.is_suppressed(0, "any"));
    }

    // A comment that stays on its line stays inactive.
    #[test]
    fn key_comment_that_stays_on_its_line_stays_inactive() {
        let input = "k: # rlsp-yaml-disable-next-line\n  v\n";
        let output = format_yaml(input, &default_opts());
        assert_eq!(output, "k: v  # rlsp-yaml-disable-next-line\n");
        let suppression = crate::validation::suppression::build_suppression_map(&output);
        assert!(!suppression.is_suppressed(1, "any"));
        assert!(!suppression.is_suppressed(0, "any"));
    }

    // The text of a comment is printed as written, at a value, at a key that
    // moves above its entry, and on a block scalar's header.
    #[rstest]
    #[case::bare_hash("#")]
    #[case::without_space("#nospace")]
    #[case::two_spaces("#  two  spaces")]
    #[case::trailing_spaces("# t  ")]
    #[case::inner_hash_and_unicode("# a # b ü 日本")]
    #[case::unbalanced_quotes("# it's \"odd")]
    #[case::sequence_text("# - x")]
    #[case::document_marker_text("# ---")]
    #[case::mapping_text("# a: b")]
    #[case::block_scalar_indicators("# |- >+")]
    #[case::nel("# a\u{85}b")]
    #[case::line_separator("# a\u{2028}b")]
    #[case::emoji("# \u{1F600}")]
    fn comment_text_is_kept_at_every_slot(#[case] comment: &str) {
        for input in [
            format!("a: v {comment}\n"),
            format!("k: {comment}\n  v # d\n"),
            format!("k: {comment}\n  sub: 1\n"),
            format!("a: | {comment}\n  t\n"),
            format!("- {comment}\n  a: 1\n"),
            format!("? k {comment}\n: v\n"),
        ] {
            assert_comments_kept(&input, &default_opts(), "default");
            let output = format_yaml(&input, &default_opts());
            assert!(
                output.contains(comment.trim_end()),
                "comment text changed: input {input:?}, output {output:?}"
            );
        }
    }

    #[test]
    fn many_key_comments_format_without_blowup() {
        let input = "k: # c\n  v # d\n".repeat(5_000);
        let output = format_yaml(&input, &default_opts());
        assert_eq!(output.lines().filter(|l| *l == "# c").count(), 5_000);
        assert_eq!(format_yaml(&output, &default_opts()), output);
    }
}
