// SPDX-License-Identifier: MIT
//
// The formatter keeps the data: whatever the option set, the output loads, holds
// the same nodes (values, tags, anchors, aliases) as the input, and formatting it
// again changes nothing.  Inputs that once lost or changed data are pinned here,
// each with the output form the formatter chooses.

#![expect(
    missing_docs,
    clippy::literal_string_with_formatting_args,
    reason = "test code; the YAML flow mappings in the inputs look like format arguments"
)]

mod loaded_data;

use rlsp_yaml::editing::editor_config::LineEnding;
use rlsp_yaml::editing::formatter::{YamlFormatOptions, format_yaml};
use rlsp_yaml::server::YamlVersion;
use rlsp_yaml_parser::node::Node;
use rlsp_yaml_parser::{DEFAULT_MAX_NESTING_DEPTH, LoaderBuilder, load};
use rstest::rstest;

// ---- helpers ----------------------------------------------------------------

/// The data of `yaml` (see `loaded_data::data_signature`); `yaml` must load.
#[expect(clippy::expect_used, reason = "test helper")]
fn data_signature(yaml: &str) -> String {
    loaded_data::data_signature(yaml).expect("the input must load")
}

/// The data of `yaml`, which must load within `max_nesting_depth`.
#[expect(clippy::expect_used, reason = "test helper")]
fn data_signature_within(yaml: &str, max_nesting_depth: usize) -> String {
    loaded_data::data_signature_within(yaml, max_nesting_depth).expect("the input must load")
}

/// Whether `yaml` loads with nesting up to `max_nesting_depth`.
fn loads_within(yaml: &str, max_nesting_depth: usize) -> bool {
    loaded_data::data_signature_within(yaml, max_nesting_depth).is_some()
}

/// Every option set the formatter's own tests run with.
fn option_sets() -> Vec<(&'static str, YamlFormatOptions)> {
    let base = YamlFormatOptions::default;
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
            "single quote, enforce block style, narrow",
            YamlFormatOptions {
                single_quote: true,
                format_enforce_block_style: true,
                print_width: 20,
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

fn options(f: impl FnOnce(&mut YamlFormatOptions)) -> YamlFormatOptions {
    let mut options = YamlFormatOptions::default();
    f(&mut options);
    options
}

/// Formatting `input` gives output that loads to the same data and does not
/// change when formatted again.
fn assert_data_kept(input: &str, options: &YamlFormatOptions, label: &str) {
    let input = in_line_ending(input, options);
    let output = format_yaml(&input, options);
    let context = format!("{label}: input {input:?}, output {output:?}");
    let limit = options.max_nesting_depth;
    assert!(
        loads_within(&output, limit),
        "the output does not load: {context}"
    );
    assert_eq!(
        data_signature_within(&output, limit),
        data_signature_within(&input, limit),
        "the data differs: {context}"
    );
    assert_eq!(
        format_yaml(&output, options),
        output,
        "the output is not stable: {context}"
    );
}

fn assert_data_kept_under_every_option_set(input: &str) {
    for (label, options) in option_sets() {
        assert_data_kept(input, &options, label);
    }
}

/// `input` with the line ends the option set's input has: `option_sets` runs
/// the CRLF set on a CRLF text.
fn in_line_ending(input: &str, options: &YamlFormatOptions) -> String {
    if options.line_ending == LineEnding::Crlf {
        input.replace('\n', "\r\n")
    } else {
        input.to_string()
    }
}

/// `assert_data_kept_under_every_option_set` for an `input` nested up to
/// `max_nesting_depth`, which every option set takes as its limit.  The
/// formatter returns a text it cannot load unchanged, which keeps the data too:
/// so each option set also has to change the text, and has to take the final
/// line break off when it is asked to.
fn assert_deep_input_is_formatted_under_every_option_set(input: &str, max_nesting_depth: usize) {
    for (label, options) in option_sets() {
        let options = YamlFormatOptions {
            max_nesting_depth,
            ..options
        };
        assert_data_kept(input, &options, label);
        let input = in_line_ending(input, &options);
        let output = format_yaml(&input, &options);
        assert_ne!(output, input, "{label}: the text came back unformatted");
        if !options.insert_final_newline {
            assert!(
                !output.ends_with(['\n', '\r']),
                "{label}: the final line break stays"
            );
        }
    }
}

fn format_default(input: &str) -> String {
    format_yaml(input, &YamlFormatOptions::default())
}

/// The root node of `yaml` as text written out by hand in the tests: scalars as
/// `'value'`, preceded by their anchor (`&a `) and by their tag unless the Core
/// schema gives it; mappings as `{key=>value, ..}`, sequences as `[item, ..]`,
/// aliases as `*name`.  It lets a test state what the output means, instead of
/// comparing the output with the input through the same loader.
#[expect(clippy::expect_used, reason = "test helper")]
fn shape_of(yaml: &str) -> String {
    fn walk(node: &Node<rlsp_yaml_parser::Span>) -> String {
        let anchor = node
            .anchor()
            .map_or_else(String::new, |name| format!("&{name} "));
        let tag = match node {
            Node::Scalar { tag, .. } | Node::Mapping { tag, .. } | Node::Sequence { tag, .. } => {
                tag.as_deref()
                    .filter(|tag| !tag.starts_with("tag:yaml.org,2002:"))
                    .map_or_else(String::new, |tag| format!("{tag} "))
            }
            Node::Alias { .. } => String::new(),
        };
        let body = match node {
            Node::Scalar { value, .. } => format!("'{value}'"),
            Node::Alias { name, .. } => format!("*{name}"),
            Node::Mapping { entries, .. } => {
                let entries: Vec<String> = entries
                    .iter()
                    .map(|(key, value)| format!("{}=>{}", walk(key), walk(value)))
                    .collect();
                format!("{{{}}}", entries.join(", "))
            }
            Node::Sequence { items, .. } => {
                let items: Vec<String> = items.iter().map(walk).collect();
                format!("[{}]", items.join(", "))
            }
        };
        format!("{anchor}{tag}{body}")
    }
    let docs = load(yaml).expect("the text must load");
    walk(&docs.first().expect("the text must hold a document").root)
}

// ---- block scalars ----------------------------------------------------------

#[rstest]
#[case::keep_with_a_blank_line_at_the_end("a: |+\n  keep\n\n")]
#[case::keep_without_a_blank_line("a: |+\n  keep\n")]
#[case::keep_before_the_next_entry("a: |+\n  x\n\n\nb: 1\n")]
#[case::keep_in_a_sequence("- |+\n  x\n\n")]
#[case::keep_without_content("a: |+\n\n")]
#[case::leading_blank_lines("a: |\n\n\n  x\n")]
#[case::interior_blank_lines("a: |\n  x\n\n\n  y\n")]
#[case::folded_with_a_leading_blank_line("c: >\n\n  y: 1\n  a: 1\n")]
#[case::folded_keep("a: >+\n  x\n\n\n")]
#[case::folded_interior_blank_lines("a: >\n  x\n\n\n  y\n")]
#[case::more_indented_lines("a: |\n  x\n\n    y\n\n  z\n")]
#[case::as_a_key("? |\n  lit\n\n: v\n")]
#[case::with_a_header_comment("a: |+ # c\n  x\n\nb: 1\n")]
#[case::whitespace_only_line_inside("a: |\n  x\n   \n  y\n")]
#[case::indentation_indicator_with_a_leading_space("a: |2\n   x\n")]
#[case::indentation_indicator_with_a_leading_blank_line("a: |2\n\n  x\n")]
#[case::indentation_indicator_of_one("a: |1\n  x\n")]
#[case::strip_without_content("a: |-\n\n")]
#[case::trailing_spaces_and_a_tab_in_lines("a: |\n  x  \n  y\t\n")]
#[case::hash_line_is_content("a: |\n  # not a comment\n  x\n")]
#[case::keep_followed_by_a_comment("a: |+\n  x\n\n# c\nb: 1\n")]
#[case::keep_in_a_key_and_a_value("? |+\n  k\n\n: |+\n  v\n\n")]
#[case::folded_long_line(
    "a: >\n  one two three four five six seven eight nine ten eleven twelve thirteen fourteen\n  mmmm\n"
)]
#[case::root_literal_with_a_percent_line("--- |\n%a\n")]
#[case::root_literal_with_a_percent_line_on_its_own_line("---\n|\n%a\n")]
fn block_scalar_keeps_its_value(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::keep_with_a_blank_line("a: |+\n  x\n\n")]
#[case::keep_with_two_blank_lines("a: |+\n  x\n\n\n")]
#[case::keep_in_a_sequence("- |+\n  x\n\n")]
#[case::folded_keep("a: >+\n  x\n\n")]
fn final_line_break_stays_when_the_last_block_scalar_needs_it(#[case] input: &str) {
    for line_ending in [LineEnding::Lf, LineEnding::Crlf, LineEnding::Cr] {
        let options = YamlFormatOptions {
            insert_final_newline: false,
            line_ending,
            ..YamlFormatOptions::default()
        };
        let terminator = match line_ending {
            LineEnding::Lf => "\n",
            LineEnding::Crlf => "\r\n",
            LineEnding::Cr => "\r",
        };
        let input = input.replace('\n', terminator);
        let output = format_yaml(&input, &options);
        assert_eq!(output, input, "{line_ending:?}");
        assert_eq!(format_yaml(&output, &options), output, "{line_ending:?}");
    }
}

#[test]
fn final_line_break_goes_when_no_value_ends_in_it() {
    let options = options(|o| o.insert_final_newline = false);
    assert_eq!(format_yaml("a: 1\n", &options), "a: 1");
    assert_eq!(format_yaml("a: |-\n  x\n", &options), "a: |-\n  x");
    assert_eq!(
        format_yaml("a: |+\n  x\n\nb: 1\n", &options),
        "a: |+\n  x\n\nb: 1"
    );
}

// ---- scalar tags and quoting ------------------------------------------------

#[rstest]
#[case::str_on_an_int("a: !!str 123\n", "a: !!str 123\n")]
#[case::str_on_a_bool("a: !!str true\n", "a: !!str true\n")]
#[case::str_on_a_hex_int("a: !!str 0x1F\n", "a: !!str 0x1F\n")]
#[case::non_specific_on_an_int("- ! 5\n", "- !!str 5\n")]
#[case::non_specific_on_a_mapping_value("k: ! 5\n", "k: !!str 5\n")]
#[case::non_specific_on_a_key("! 5: v\n", "!!str 5: v\n")]
#[case::int_on_a_quoted_string("a: !!int \"5\"\n", "a: !!int \"5\"\n")]
#[case::int_on_a_word("a: !!int x\n", "a: !!int x\n")]
#[case::float_on_an_int("a: !!float 1\n", "a: !!float 1\n")]
#[case::str_on_a_word_is_implied("a: !!str hello\n", "a: hello\n")]
#[case::str_on_a_quoted_string_is_implied("a: !!str \"hello\"\n", "a: hello\n")]
#[case::int_on_an_int_is_implied("a: !!int 5\n", "a: 5\n")]
#[case::non_specific_on_a_word_is_implied("- ! x\n", "- x\n")]
#[case::str_in_a_flow_sequence("[!!str 5, b]\n", "[!!str 5, b]\n")]
#[case::str_on_a_flow_mapping_key("{!!str 5: v}\n", "{ !!str 5: v }\n")]
#[case::anchor_and_str("a: &x !!str 5\n", "a: &x !!str 5\n")]
#[case::str_on_a_root_scalar("!!str 123\n", "!!str 123\n")]
#[case::str_on_a_root_scalar_after_the_marker("--- !!str 123\n", "---\n!!str 123\n")]
fn explicit_scalar_tag_stays_when_the_value_would_read_as_another_type(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::hex("a: \"0x1F\"\n", "a: \"0x1F\"\n")]
#[case::octal("a: '0o17'\n", "a: '0o17'\n")]
#[case::signed_infinity("a: \"-.INF\"\n", "a: \"-.INF\"\n")]
#[case::decimal("a: \"123\"\n", "a: \"123\"\n")]
#[case::float("a: '1.5e3'\n", "a: '1.5e3'\n")]
fn quoted_string_that_reads_as_a_number_stays_quoted(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::hex("a: 0x1F\n", "0x1F")]
#[case::octal("a: 0o17\n", "0o17")]
#[case::signed_infinity("a: -.INF\n", "-.INF")]
#[case::float("a: 1.5e3\n", "1.5e3")]
#[case::null_word("a: Null\n", "Null")]
#[case::in_a_flow_sequence("[0x1F, b]\n", "0x1F")]
fn plain_number_stays_plain_with_single_quote(#[case] input: &str, #[case] number: &str) {
    let options = options(|o| o.single_quote = true);
    let output = format_yaml(input, &options);
    assert!(
        output.contains(number) && !output.contains(&format!("'{number}'")),
        "the value is quoted: input {input:?}, output {output:?}"
    );
    assert_data_kept(input, &options, "single quote");
    assert_data_kept_under_every_option_set(input);
}

#[test]
fn single_quote_option_does_not_break_a_value_with_an_apostrophe() {
    let input = "a: 'here''s'\nb: \"it's\"\n";
    let options = options(|o| o.single_quote = true);
    let output = format_yaml(input, &options);
    assert_eq!(output, "a: \"here's\"\nb: \"it's\"\n");
    assert_data_kept(input, &options, "single quote");
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::mapping_value("k: it's\n", "k: \"it's\"\n")]
#[case::sequence_item("- it's\n", "- \"it's\"\n")]
#[case::flow_item("[it's, b]\n", "[\"it's\", 'b']\n")]
#[case::plain_without_apostrophe("k: v\n", "k: 'v'\n")]
fn single_quote_prefers_double_quotes_for_a_value_with_an_apostrophe(
    #[case] input: &str,
    #[case] expected: &str,
) {
    let options = options(|o| o.single_quote = true);
    assert_eq!(format_yaml(input, &options), expected);
}

#[rstest]
#[case::double_quoted("\"\": v\n", "\"\": v\n")]
#[case::single_quoted("'': v\n", "'': v\n")]
#[case::as_a_key_with_an_empty_value("\"\":\n", "\"\": \n")]
#[case::in_a_flow_mapping("{\"\": v}\n", "{ \"\": v }\n")]
fn empty_string_key_stays_an_empty_string(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

// ---- properties -------------------------------------------------------------

#[rstest]
#[case::in_a_sequence("- &a\n- b\n")]
#[case::as_a_mapping_value("a: &x\nb: 1\n")]
#[case::as_a_key_and_a_value("? &a\n: &b\n")]
#[case::before_a_comment("- &a # c\n- b\n")]
#[case::with_a_tag("- &a !!str\n- &b !t\n")]
#[case::with_a_core_null_tag("k: &a !!null\n")]
#[case::in_a_flow_mapping("{&a : 1}\n")]
#[case::as_a_flow_mapping_key_and_alias_value("{&a : *a}\n")]
#[case::empty_string("- &a \"\"\n- &b ''\n")]
fn anchor_on_an_empty_scalar_stays(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

// A key with no text, or with properties, reads as its own entry wherever it
// stands, so it prints without `? ` after a value with no text and as the first
// key of a mapping that is the value of an entry.  An alias key never needed
// `? ` there.
#[rstest]
#[case::tagged_empty_key("a:\n!t : 1\n", "a: \n!t : 1\n", "{'a'=>'', !t ''=>'1'}")]
#[case::anchored_empty_key("a:\n&x : 1\n", "a: \n&x : 1\n", "{'a'=>'', &x ''=>'1'}")]
#[case::empty_key("a:\n: 1\n", "a: \n: 1\n", "{'a'=>'', ''=>'1'}")]
#[case::tagged_scalar_key("a:\n!t b: 1\n", "a: \n!t b: 1\n", "{'a'=>'', !t 'b'=>'1'}")]
#[case::anchored_scalar_key("a:\n&y b: 1\n", "a: \n&y b: 1\n", "{'a'=>'', &y 'b'=>'1'}")]
#[case::tagged_empty_key_after_a_tagged_value(
    "a: !t\n!u : 1\n",
    "a: !t\n!u : 1\n",
    "{'a'=>!t '', !u ''=>'1'}"
)]
#[case::anchored_empty_key_after_a_tagged_value(
    "a: !t\n&x : 1\n",
    "a: !t\n&x : 1\n",
    "{'a'=>!t '', &x ''=>'1'}"
)]
#[case::anchored_empty_key_after_an_anchored_value(
    "a: &y\n&x : 1\n",
    "a: &y\n&x : 1\n",
    "{'a'=>&y '', &x ''=>'1'}"
)]
#[case::anchored_scalar_key_after_a_tagged_value(
    "a: !t\n&x b: 1\n",
    "a: !t\n&x b: 1\n",
    "{'a'=>!t '', &x 'b'=>'1'}"
)]
#[case::tagged_empty_key_after_a_quoted_empty_value(
    "a: !!str \"\"\n!t : 1\n",
    "a: !!str \"\"\n!t : 1\n",
    "{'a'=>'', !t ''=>'1'}"
)]
#[case::tagged_empty_key_with_an_empty_value(
    "a:\n!t :\nb: 1\n",
    "a: \n!t : \nb: 1\n",
    "{'a'=>'', !t ''=>'', 'b'=>'1'}"
)]
#[case::two_tagged_empty_keys(
    "a:\n!t : 1\n!u : 2\n",
    "a: \n!t : 1\n!u : 2\n",
    "{'a'=>'', !t ''=>'1', !u ''=>'2'}"
)]
#[case::tagged_empty_key_first_in_a_value(
    "k:\n  !t : 1\n",
    "k:\n  !t : 1\n",
    "{'k'=>{!t ''=>'1'}}"
)]
#[case::anchored_empty_key_first_in_a_value(
    "k:\n  &x : 1\n",
    "k:\n  &x : 1\n",
    "{'k'=>{&x ''=>'1'}}"
)]
#[case::empty_key_first_in_a_value("k:\n  : 1\n", "k:\n  : 1\n", "{'k'=>{''=>'1'}}")]
#[case::empty_key_first_in_an_anchored_value(
    "k: &m\n  : 1\n",
    "k: &m\n  : 1\n",
    "{'k'=>&m {''=>'1'}}"
)]
#[case::tagged_empty_key_first_in_an_anchored_value(
    "k: &m\n  !t : 1\n",
    "k: &m\n  !t : 1\n",
    "{'k'=>&m {!t ''=>'1'}}"
)]
#[case::empty_key_first_in_a_value_in_a_sequence(
    "- k:\n    : 1\n",
    "- k:\n    : 1\n",
    "[{'k'=>{''=>'1'}}]"
)]
#[case::empty_key_first_in_the_value_of_an_explicit_entry(
    "? a\n: \n  : 1\n",
    "a:\n  : 1\n",
    "{'a'=>{''=>'1'}}"
)]
#[case::tagged_empty_key_later_in_a_value(
    "k:\n  a:\n  !t : 2\n",
    "k:\n  a: \n  !t : 2\n",
    "{'k'=>{'a'=>'', !t ''=>'2'}}"
)]
#[case::alias_key_after_an_anchored_empty_value(
    "x: &z v\na: &y\n*z : 1\n",
    "x: &z v\na: &y\n*z : 1\n",
    "{'x'=>&z 'v', 'a'=>&y '', *z=>'1'}"
)]
fn key_with_no_text_prints_without_a_question_mark(
    #[case] input: &str,
    #[case] expected: &str,
    #[case] shape: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_eq!(shape_of(expected), shape);
    assert_data_kept_under_every_option_set(input);
}

// The option that removes duplicate keys keeps one of the two, so it is left out.
#[test]
fn two_empty_keys_in_a_value_stay_two_entries() {
    let input = "k:\n  : 1\n  : 2\n";
    assert_eq!(format_default(input), input);
    assert_eq!(shape_of(input), "{'k'=>{''=>'1', ''=>'2'}}");
    for (label, options) in option_sets() {
        if !options.format_remove_duplicate_keys {
            assert_data_kept(input, &options, label);
        }
    }
}

// A collection key with properties after a value with none, or below the
// properties of its mapping, keeps `? `.
#[rstest]
#[case::tagged_sequence_key("a:\n!t [x] : 1\n", "a: \n? !t [x]\n: 1\n", "{'a'=>'', !t ['x']=>'1'}")]
#[case::anchored_mapping_key(
    "a:\n&x {p: 1} : 1\n",
    "a: \n? &x { p: 1 }\n: 1\n",
    "{'a'=>'', &x {'p'=>'1'}=>'1'}"
)]
#[case::key_below_the_properties_of_its_mapping(
    "k: &m\n  ? [a]\n  : 1\n",
    "k: &m\n  ? [a]\n  : 1\n",
    "{'k'=>&m {['a']=>'1'}}"
)]
fn collection_key_with_properties_keeps_its_question_mark(
    #[case] input: &str,
    #[case] expected: &str,
    #[case] shape: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_eq!(shape_of(expected), shape);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::empty_key_in_a_flow_mapping("k: {: 1}\n")]
#[case::tagged_empty_key_in_a_flow_mapping("k: {!t : 1}\n")]
#[case::flow_mapping_in_a_flow_sequence("[a, {: 1}]\n")]
#[case::empty_key_first_in_a_block_mapping_key("? k:\n    : 1\n: v\n")]
#[case::tagged_empty_key_first_in_a_block_mapping_key("? k:\n    !t : 1\n: v\n")]
#[case::tagged_sequence_key_in_a_block_mapping_key("? a:\n  !t [x] : 1\n: v\n")]
fn key_with_no_text_in_a_collection_keeps_its_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::anchor_and_tag("x: &a 1\n*a : &y\n  b: 1\n", "x: &a 1\n*a : &y\n  b: 1\n")]
#[case::tag_and_sequence("x: &a 1\n*a : !t\n  - 1\n", "x: &a 1\n*a : !t\n  - 1\n")]
fn alias_key_keeps_the_space_before_its_colon(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::flow_sequence("- ! [a, b]\n", "- ! [a, b]\n")]
#[case::flow_mapping("- ! {a: 1}\n", "- ! { a: 1 }\n")]
#[case::block_sequence("k: !\n  - a\n", "k: !\n  - a\n")]
#[case::block_mapping("- !\n  a: 1\n", "- !\n  a: 1\n")]
#[case::with_an_anchor("&a ! [a]\n", "&a ! [a]\n")]
#[case::as_a_key("! [a]: v\n", "? ! [a]\n: v\n")]
#[case::core_tag_of_the_kind_is_implied("- !!seq [a]\n- !!map {a: 1}\n", "- [a]\n- { a: 1 }\n")]
fn non_specific_tag_on_a_collection_stays(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::anchored_empty_mapping("- &b !t {}\n")]
#[case::anchored_empty_sequence("k: &a []\n")]
#[case::tagged_empty_mapping_key("!t {}: v\n")]
fn properties_of_an_empty_collection_stay_on_its_line(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
    let options = options(|o| o.format_enforce_block_style = true);
    let output = format_yaml(input, &options);
    assert!(
        output.contains("{}") || output.contains("[]"),
        "the empty collection moved: {output:?}"
    );
    assert!(
        !output.contains("\n{}") && !output.contains("\n[]"),
        "the empty collection is on a line of its own: {output:?}"
    );
}

#[rstest]
#[case::set("s: !!set\n  ? a\n  ? b\n", "s: !!set\n  a: \n  b: \n")]
#[case::omap("o: !!omap\n  - a: 1\n", "o: !!omap\n  - a: 1\n")]
#[case::flow_set("!!set {a, b}\n", "!!set { a: , b:  }\n")]
#[case::str_on_a_sequence("- !!str [a]\n", "- !!str [a]\n")]
#[case::set_as_a_key("? !!set {a}\n: !!omap [b]\n", "? !!set { a:  }\n: !!omap [b]\n")]
#[case::anchor_and_set("o: &x !!set {a}\n", "o: &x !!set { a:  }\n")]
#[case::map_on_a_mapping_is_implied("- !!map {a: 1}\n- !!seq [a]\n", "- { a: 1 }\n- [a]\n")]
fn core_tag_of_another_type_stays_on_a_collection(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

// ---- flow mappings and sequences --------------------------------------------

#[rstest]
#[case::only_entry("{c: }\n")]
#[case::in_a_sequence("[{c: }, {d: }]\n")]
#[case::in_a_mapping("k: {c: , d: }\n")]
#[case::in_a_sequence_item("- {c: }\n")]
#[case::empty_value_with_an_anchor("{c: &a }\n")]
fn flow_mapping_entry_without_a_value_keeps_its_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::null_tag("[!!null \"\", b]\n", "[!!null \"\", b]\n")]
#[case::custom_tag("[!t \"\", b]\n", "[!t \"\", b]\n")]
#[case::anchor_and_null_tag("[&a !!null \"\", b]\n", "[&a !!null \"\", b]\n")]
#[case::anchored_empty_string("[&a \"\", b]\n", "[&a \"\", b]\n")]
#[case::last_item("[a, !!null \"\"]\n", "[a, !!null \"\"]\n")]
fn empty_item_of_a_flow_sequence_prints_its_empty_string(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

// ---- collection keys --------------------------------------------------------

#[rstest]
#[case::default(YamlFormatOptions::default())]
#[case::enforce_block_style(YamlFormatOptions {
    format_enforce_block_style: true,
    ..YamlFormatOptions::default()
})]
fn mapping_key_that_holds_an_explicit_entry_prints_in_block_form(
    #[case] options: YamlFormatOptions,
) {
    let input = "? [a]: x\n";
    let expected = "? ? [a]\n  : x\n:\n";
    assert_eq!(format_yaml(input, &options), expected);
    assert_eq!(shape_of(expected), "{{['a']=>'x'}=>''}");
    assert_data_kept(input, &options, "key");
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::flow_mapping_after_the_first_entry("x: 1\n{a: 1}: v\n", "x: 1\n? { a: 1 }\n: v\n")]
#[case::flow_sequence_after_the_first_entry("x: 1\n[a, b]: v\n", "x: 1\n? [a, b]\n: v\n")]
#[case::flow_mapping_as_the_first_entry("{a: 1}: v\n", "? { a: 1 }\n: v\n")]
#[case::flow_sequence_as_the_first_entry("[a]: v\n", "? [a]\n: v\n")]
#[case::in_a_sequence_item("- {a: 1}: v\n", "- ? { a: 1 }\n  : v\n")]
fn flow_collection_key_stays_flow_under_enforce_block_style(
    #[case] input: &str,
    #[case] expected: &str,
) {
    let options = options(|o| o.format_enforce_block_style = true);
    assert_eq!(format_yaml(input, &options), expected);
    assert_data_kept(input, &options, "enforce block style");
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::mapping_after_the_first_entry(
    "x: 1\n? a: 1\n: v\n",
    "x: 1\n? a: 1\n: v\n",
    "{'x'=>'1', {'a'=>'1'}=>'v'}"
)]
#[case::mapping_with_two_entries_after_the_first_entry(
    "x: 1\n? a: 1\n  b: 2\n: v\n",
    "x: 1\n? a: 1\n  b: 2\n: v\n",
    "{'x'=>'1', {'a'=>'1', 'b'=>'2'}=>'v'}"
)]
#[case::sequence_after_the_first_entry(
    "x: 1\n? - a\n  - b\n: v\n",
    "x: 1\n? - a\n  - b\n: v\n",
    "{'x'=>'1', ['a', 'b']=>'v'}"
)]
#[case::mapping_after_the_first_entry_in_a_value(
    "k:\n  x: 1\n  ? a: 1\n  : v\n",
    "k:\n  x: 1\n  ? a: 1\n  : v\n",
    "{'k'=>{'x'=>'1', {'a'=>'1'}=>'v'}}"
)]
#[case::mapping_after_the_first_entry_in_a_sequence_item(
    "- x: 1\n  ? a: 1\n  : v\n",
    "- x: 1\n  ? a: 1\n  : v\n",
    "[{'x'=>'1', {'a'=>'1'}=>'v'}]"
)]
#[case::mapping_as_the_first_entry_in_a_sequence_item(
    "- ? a: 1\n  : v\n",
    "- ? a: 1\n  : v\n",
    "[{{'a'=>'1'}=>'v'}]"
)]
#[case::two_mappings_after_the_first_entry(
    "x: 1\n? a: 1\n: v\ny: 2\n? b: 2\n: w\n",
    "x: 1\n? a: 1\n: v\ny: 2\n? b: 2\n: w\n",
    "{'x'=>'1', {'a'=>'1'}=>'v', 'y'=>'2', {'b'=>'2'}=>'w'}"
)]
#[case::sequence_key_that_holds_an_explicit_entry(
    "? - [b]: c\n: d\n",
    "? - ? [b]\n    : c\n: d\n",
    "{[{['b']=>'c'}]=>'d'}"
)]
#[case::two_explicit_entries_in_a_key_after_the_first_entry(
    "x: 1\n? [a]: x\n  [b]: y\n: v\n",
    "x: 1\n? ? [a]\n  : x\n  ? [b]\n  : y\n: v\n",
    "{'x'=>'1', {['a']=>'x', ['b']=>'y'}=>'v'}"
)]
#[case::key_that_holds_an_explicit_entry_below_the_anchor_of_the_root(
    "&m\nx: 1\n? [a]: 1\n: v\n",
    "&m\nx: 1\n? ? [a]\n  : 1\n: v\n",
    "&m {'x'=>'1', {['a']=>'1'}=>'v'}"
)]
#[case::nested_explicit_key_with_a_scalar_key_prints_compact(
    "? ? a\n  : b\n: v\n",
    "? a: b\n: v\n",
    "{{'a'=>'b'}=>'v'}"
)]
#[case::nested_explicit_key_with_a_flow_key_is_a_fixed_point(
    "? ? [a]\n  : x\n:\n",
    "? ? [a]\n  : x\n:\n",
    "{{['a']=>'x'}=>''}"
)]
#[case::anchor_on_a_mapping_key(
    "x: 1\n? &k\n  a: 1\n: v\n",
    "x: 1\n? &k\n  a: 1\n: v\n",
    "{'x'=>'1', &k {'a'=>'1'}=>'v'}"
)]
#[case::tag_on_a_mapping_key(
    "x: 1\n? !t\n  a: 1\n: v\n",
    "x: 1\n? !t\n  a: 1\n: v\n",
    "{'x'=>'1', !t {'a'=>'1'}=>'v'}"
)]
#[case::anchor_and_tag_on_a_sequence_key(
    "x: 1\n? &k !t\n  - a\n: v\n",
    "x: 1\n? &k !t\n  - a\n: v\n",
    "{'x'=>'1', &k !t ['a']=>'v'}"
)]
#[case::properties_on_the_root_and_on_its_first_key(
    "&m\n? &k\n  a: 1\n: v\n",
    "&m\n? &k\n  a: 1\n: v\n",
    "&m {&k {'a'=>'1'}=>'v'}"
)]
#[case::core_tag_on_a_mapping_key_is_dropped(
    "x: 1\n? !!map\n  a: 1\n: v\n",
    "x: 1\n? a: 1\n: v\n",
    "{'x'=>'1', {'a'=>'1'}=>'v'}"
)]
#[case::block_scalars_in_a_mapping_key_stay_block_scalars(
    "x: 1\n? a: |\n    lit\n  b: >\n    fold\n: v\n",
    "x: 1\n? a: |\n    lit\n  b: >\n    fold\n: v\n",
    "{'x'=>'1', {'a'=>'lit\n', 'b'=>'fold\n'}=>'v'}"
)]
#[case::block_scalar_in_a_mapping_key_as_the_first_entry(
    "? a: |\n    lit\n  b: 2\n: v\n",
    "? a: |\n    lit\n  b: 2\n: v\n",
    "{{'a'=>'lit\n', 'b'=>'2'}=>'v'}"
)]
fn collection_key_prints_in_block_form(
    #[case] input: &str,
    #[case] expected: &str,
    #[case] shape: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_eq!(
        format_yaml(input, &options(|o| o.format_enforce_block_style = true)),
        expected
    );
    assert_eq!(shape_of(expected), shape);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::sequence_value(
    "x: 1\n? a: [1, 2]\n: v\n",
    "x: 1\n? a: [1, 2]\n: v\n",
    "x: 1\n? a:\n    - 1\n    - 2\n: v\n"
)]
#[case::mapping_and_sequence_values(
    "x: 1\n? a: {b: 1}\n  c: [d]\n: v\n",
    "x: 1\n? a: { b: 1 }\n  c: [d]\n: v\n",
    "x: 1\n? a:\n    b: 1\n  c:\n    - d\n: v\n"
)]
fn flow_value_in_a_block_key_follows_the_enforce_block_style_option(
    #[case] input: &str,
    #[case] by_default: &str,
    #[case] enforced: &str,
) {
    assert_eq!(format_default(input), by_default);
    assert_eq!(
        format_yaml(input, &options(|o| o.format_enforce_block_style = true)),
        enforced
    );
    assert_data_kept_under_every_option_set(input);
}

#[test]
fn flow_items_of_a_sequence_key_keep_their_data_under_every_option_set() {
    assert_data_kept_under_every_option_set("x: 1\n? - {a: 1}\n  - [b]\n: v\n");
}

#[rstest]
#[case::flow_mapping_after_the_first_entry("x: 1\n{a: 1}: v\n", "x: 1\n? { a: 1 }\n: v\n")]
#[case::flow_mapping_with_a_question_mark("x: 1\n? {a: 1}\n: v\n", "x: 1\n? { a: 1 }\n: v\n")]
#[case::flow_sequence_after_the_first_entry("x: 1\n[a, b]: v\n", "x: 1\n? [a, b]\n: v\n")]
#[case::flow_mapping_as_the_first_entry("{a: 1}: v\n", "? { a: 1 }\n: v\n")]
#[case::flow_key_inside_a_block_key(
    "x: 1\n? a: 1\n  ? [b]\n  : y\n: v\n",
    "x: 1\n? a: 1\n  ? [b]\n  : y\n: v\n"
)]
fn flow_collection_key_keeps_its_flow_style(#[case] input: &str, #[case] expected: &str) {
    assert_eq!(format_default(input), expected);
    assert_eq!(
        format_yaml(input, &options(|o| o.format_enforce_block_style = true)),
        expected
    );
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::comments_after_the_values(
    "x: 1\n? a: 1 # c1\n  b: 2 # c2\n: v # c3\n",
    "x: 1\n? a: 1  # c1\n  b: 2  # c2\n: v  # c3\n"
)]
#[case::comments_after_the_items_of_a_sequence_key(
    "x: 1\n? - a # ia\n  - b # ib\n: v\n",
    "x: 1\n? - a  # ia\n  - b  # ib\n: v\n"
)]
#[case::comment_on_its_own_line_between_entries(
    "x: 1\n? a: 1\n  # inner own line\n  b: 2\n: v\n",
    "x: 1\n? a: 1\n  # inner own line\n  b: 2\n: v\n"
)]
#[case::comment_above_the_key(
    "x: 1\n# above key\n? a: 1\n: v\n",
    "x: 1\n# above key\n? a: 1\n: v\n"
)]
#[case::comment_on_the_question_mark_line(
    "x: 1\n? # lead\n  a: 1 # t\n  b: 2\n: v\n",
    "x: 1\n# lead\n? a: 1  # t\n  b: 2\n: v\n"
)]
#[case::comment_on_the_properties_line(
    "x: 1\n? &k # c\n  a: 1\n: v\n",
    "x: 1\n# c\n? &k\n  a: 1\n: v\n"
)]
#[case::comments_in_a_nested_explicit_entry(
    "? # c0\n  ? [a] # c1\n  : x # c2\n: v # c3\n",
    "# c0\n? ? [a]  # c1\n  : x  # c2\n: v  # c3\n"
)]
#[case::comments_in_a_nested_explicit_entry_in_a_sequence_key(
    "x: 1\n? # c0\n  - ? [a] # c1\n    : x # c2\n: v # c3\n",
    "x: 1\n# c0\n? - ? [a]  # c1\n    : x  # c2\n: v  # c3\n"
)]
#[case::comment_in_a_flow_key_inside_the_key(
    "x: 1\n? a: 1\n  ? [b, # c\n     d]\n  : x\n: v\n",
    "x: 1\n? a: 1\n  # c\n  ? [b, d]\n  : x\n: v\n"
)]
#[case::comment_in_a_flow_key_at_the_start_of_the_key(
    "x: 1\n? ? [b, # c\n     d]\n  : x\n: v\n",
    "x: 1\n# c\n? ? [b, d]\n  : x\n: v\n"
)]
#[case::comment_in_a_flow_key_of_the_mapping(
    "x: 1\n? { a: 1, # c\n    b: 2 }\n: v\n",
    "x: 1\n# c\n? { a: 1, b: 2 }\n: v\n"
)]
fn comments_inside_a_collection_key_print_where_their_nodes_print(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

#[test]
fn key_inside_a_block_key_whose_printed_form_exceeds_an_implicit_key_keeps_its_question_mark() {
    // 600 characters in the input, 1200 once each U+2028 is written as `\L`.
    let long = "\u{2028}".repeat(600);
    let input = format!("x: 1\n? \"{long}\": 1\n  y: 2\n: v\n");
    assert!(load(&input).is_ok(), "the input must load");
    assert_data_kept_under_every_option_set(&input);
}

#[test]
fn duplicate_block_keys_with_comments_keep_their_comments() {
    let input = "x: 1\n? a: 1 # c1\n  b: 2\n: v # c2\n? a: 1 # c3\n  b: 2\n: w # c4\n";
    let options = options(|o| o.format_remove_duplicate_keys = true);
    let output = format_yaml(input, &options);
    assert!(load(&output).is_ok(), "the output must load: {output:?}");
    for comment in ["# c1", "# c2", "# c3", "# c4"] {
        assert_eq!(
            output.matches(comment).count(),
            1,
            "{comment} in {output:?}"
        );
    }
    assert_eq!(format_yaml(&output, &options), output);
}

// ---- explicit keys that hold explicit keys ----------------------------------

#[rstest]
#[case::mapping_in_a_mapping_key("? a:\n    [b]: c\n: d\n")]
#[case::sequence_key_holding_an_explicit_entry("? - [b]: c\n: d\n")]
#[case::empty_item_in_a_sequence_key("? - [x]: !!str null\n  - 5\n  -\n:\n")]
#[case::several_entries("? [a]: x\n  [b]: y\n")]
#[case::as_a_mapping_value_key("k:\n  ? [a]: x\n")]
#[case::second_key_of_a_mapping("z: 1\n? [a]: x\n")]
#[case::block_scalar_inside("? a: 1\n  [b]: |\n    lit\n: v\n")]
#[case::comments_inside("? a: 1 # c1\n  [b]: x # c2\n: v # c3\n")]
#[case::nested_explicit_key_with_an_empty_value("?\n  ? a\n: v\n")]
#[case::inline_nested_explicit_key("? ? a\n: v\n")]
#[case::nested_empty_explicit_key("?\n  ?\n: v\n")]
#[case::inline_nested_empty_explicit_key("? ?\n: v\n")]
#[case::anchor_on_the_outer_explicit_key_of_a_nested_one("? &a\n  ? a\n: v\n")]
#[case::anchor_on_the_empty_inner_explicit_key("?\n  ? &a\n: v\n")]
#[case::flow_sequence_as_nested_explicit_key("?\n  ? [a]\n: v\n")]
#[case::alias_as_nested_explicit_key("?\n  ? *x\n: v\n")]
#[case::nested_explicit_key_in_a_sequence_key("?\n  - ? a\n: v\n")]
#[case::nested_explicit_key_in_an_inline_sequence_key("? - ? a\n: v\n")]
#[case::nested_explicit_key_in_a_same_column_sequence_key("?\n- ? a\n: v\n")]
#[case::nested_explicit_key_in_the_second_item_of_a_sequence_key("?\n- a\n- ? b\n: v\n")]
#[case::explicit_key_nested_in_an_item_with_a_value("- ?\n    ? a\n  : v\n")]
#[case::explicit_key_nested_in_a_mapping_value("x:\n  ?\n    ? a\n  : v\n")]
#[case::explicit_key_after_an_implicit_entry_in_a_key_mapping("?\n  a: 1\n  ? b\n: v\n")]
#[case::explicit_key_with_a_value_after_a_nested_explicit_key("?\n  ? a\n  : b\n  ? c\n: v\n")]
#[case::nested_explicit_key_under_an_implicit_key_in_a_key("?\n  x:\n    ? a\n: v\n")]
#[case::nested_explicit_key_in_a_sequence_under_an_implicit_key_in_a_key("?\n  x:\n  - ? a\n: v\n")]
#[case::two_inline_nested_explicit_keys("?\n  ? ? a\n: v\n")]
#[case::two_nested_explicit_keys_over_lines("?\n  ?\n    ? a\n: v\n")]
#[case::nested_explicit_key_with_an_empty_outer_value("?\n  ? a\n:\n")]
#[case::nested_explicit_key_at_the_end_of_the_input("?\n  ? a\n")]
#[case::nested_explicit_key_with_a_value_at_the_end_of_the_input("?\n  ? a\n  : b\n")]
#[case::nested_explicit_key_before_a_document_end("?\n  ? a\n...\n")]
#[case::nested_explicit_key_before_a_document_start("?\n  ? a\n---\nb\n")]
#[case::nested_explicit_key_before_an_explicit_key("?\n  ? a\n? b\n")]
#[case::nested_explicit_key_before_a_plain_key("?\n  ? a\nb: 1\n")]
#[case::nested_explicit_key_before_an_alias_key("?\n  ? a\n*x : 1\n")]
#[case::nested_explicit_key_before_a_flow_key("?\n  ? a\n[x]: 1\n")]
#[case::nested_explicit_key_before_an_anchored_empty_key("?\n  ? a\n&k : v\n")]
#[case::nested_explicit_key_in_an_item_before_the_next_item("- ?\n    ? a\n- b\n")]
#[case::explicit_key_then_a_key_that_holds_a_mapping("? a\n?\n  b: 1\n")]
#[case::explicit_key_then_a_compact_key_mapping("? a\n? b: 1\n: v\n")]
#[case::entry_then_an_explicit_key_that_holds_a_mapping("a: 1\n?\n  b: 2\n: v\n")]
#[case::explicit_entry_then_a_compact_key_mapping("? a\n: b\n? c: d\n: e\n")]
#[case::explicit_key_then_a_key_that_holds_an_explicit_key("? a\n?\n  ? b\n: v\n")]
#[case::entry_then_a_compact_key_mapping("a: 1\n? b: 2\n: c\n")]
#[case::three_inline_nested_explicit_keys_with_an_empty_value("? ? ? ? a\n: v\n")]
#[case::three_nested_empty_explicit_keys("?\n  ?\n  ?\n: v\n")]
#[case::four_nested_explicit_keys("?\n  ?\n    ?\n      ? a\n: v\n")]
#[case::nested_explicit_key_then_a_second_one("? ? a\n  ? b\n: v\n")]
#[case::nested_explicit_key_then_an_outer_key("? ? a\n? b\n: v\n")]
#[case::explicit_key_then_a_key_with_two_nested_ones("? a\n?\n  ? b\n  ? c\n: v\n")]
#[case::entry_then_a_key_mapping_with_two_entries("a: 1\n? b: 2\n  c: 3\n: v\n")]
#[case::item_mapping_then_an_explicit_key_that_holds_a_mapping("- a: 1\n  ?\n    b: 2\n  : v\n")]
#[case::item_then_an_item_with_an_explicit_key_that_holds_a_mapping("- x\n- ?\n    a: 1\n  : v\n")]
#[case::nested_explicit_key_that_holds_an_empty_entry("? ?\n  : v\n")]
#[case::nested_explicit_key_with_a_literal_scalar("?\n  ? |\n    lit\n: v\n")]
#[case::inline_nested_explicit_key_with_a_literal_scalar("? ? |\n    x\n: v\n")]
#[case::nested_explicit_key_with_a_multiline_scalar("?\n  ? a\n    b\n: v\n")]
#[case::nested_explicit_key_with_a_quoted_scalar("?\n  ? \"a\"\n: 'v'\n")]
#[case::nested_explicit_key_with_a_flow_mapping("?\n  ? {a: b}\n: v\n")]
#[case::inline_nested_explicit_key_with_a_flow_sequence("? ? [a, b]\n: v\n")]
#[case::tag_on_the_outer_explicit_key_of_a_nested_one("? !!map\n  ? a\n: v\n")]
#[case::tag_on_the_scalar_of_a_nested_explicit_key("?\n  ? !!str x\n: v\n")]
#[case::anchor_on_the_outer_key_and_an_alias_value("? &m\n  ? a\n: *m\n")]
#[case::nested_explicit_key_then_a_sequence_under_an_implicit_key("?\n  x:\n  - a\n  ? b\n: v\n")]
#[case::nested_explicit_key_with_a_sequence_value("?\n  ? a\n:\n  - x\n")]
#[case::nested_explicit_key_with_a_mapping_value("?\n  ? a\n:\n  k: x\n")]
#[case::nested_explicit_key_then_another_entry("?\n  ? a\n: v\n? b\n: c\n")]
#[case::nested_explicit_key_then_a_plain_entry("?\n  ? a\n: v\nz: 1\n")]
#[case::nested_explicit_key_with_an_empty_value_then_another_entry("?\n  ? a\n:\n? b\n: c\n")]
#[case::nested_explicit_keys_in_two_documents("---\n?\n  ? a\n---\n?\n  ? b\n...\n")]
#[case::nested_explicit_key_with_an_empty_value_before_a_document_end(
    "?\n  ? a\n:\n...\n?\n  ? b\n"
)]
#[case::nested_explicit_key_without_a_line_break("?\n  ? a")]
#[case::inline_nested_explicit_key_without_a_line_break("? ? a")]
#[case::nested_empty_explicit_key_without_a_line_break("?\n  ?")]
#[case::inline_nested_empty_explicit_key_without_a_line_break("? ?")]
#[case::nested_explicit_key_with_a_bare_colon_at_the_end("? ? a\n:")]
#[case::nested_explicit_key_with_a_value_at_the_end("? ? a\n: v")]
#[case::tag_on_the_root_before_a_nested_explicit_key("!!map\n? ? a\n: v\n")]
#[case::anchor_on_the_root_before_a_nested_explicit_key("&r\n?\n  ? a\n: v\n")]
#[case::tag_after_the_marker_before_a_nested_explicit_key("--- !!map\n?\n  ? a\n: v\n")]
#[case::nested_explicit_key_with_crlf("?\r\n  ? a\r\n: v\r\n")]
#[case::nested_explicit_key_with_a_tab_before_the_value("?\n  ? a\n:\tv\n")]
#[case::nested_explicit_key_with_a_tab_before_the_content("?\n  ?\ta\n: v\n")]
#[case::explicit_key_as_the_value_of_an_explicit_key("? a\n: ? b\n  : c\n? d\n: e\n")]
#[case::explicit_key_as_the_value_in_an_item("- ? a\n  : ? b\n- c\n")]
#[case::sequence_with_an_explicit_key_as_the_value("? a\n: - ? b\n    : c\n? d\n: e\n")]
#[case::bare_explicit_key_then_a_colon_line("?\n: v\n")]
#[case::bare_explicit_key_with_a_sequence_key("?\n- a\n: v\n")]
#[case::explicit_key_with_a_compact_empty_key("? : v")]
#[case::explicit_key_with_a_block_mapping("?\n  a: 1\n: v\n")]
#[case::explicit_key_with_a_block_sequence("?\n  - a\n: v\n")]
#[case::explicit_key_with_an_alias_key_entry("?\n  *x :\n: v\n")]
#[case::explicit_key_then_an_alias_key("? a\n? b\n*y : 3\n")]
#[case::value_after_a_nested_entry_with_a_value("?\n  ? a\n  : b\n: v\n")]
#[case::value_after_an_inline_nested_entry_with_a_value("? ? a\n  : b\n: v\n")]
#[case::value_after_a_block_mapping_key_ending_in_an_entry("?\n  x: 1\n  ? a\n  : b\n: v\n")]
#[case::value_after_a_sequence_key_ending_in_an_entry("?\n  - ? a\n    : b\n: v\n")]
#[case::empty_value_after_a_nested_entry_with_a_value("?\n  ? a\n  : b\n:\n")]
#[case::value_after_an_empty_key_entry_in_a_key("?\n  ? a\n  : b\n  : c\n: v\n")]
#[case::value_after_three_levels("? ? ? a\n    : x\n  : y\n: z\n")]
#[case::value_after_an_anchored_nested_entry("?\n  ? &a x\n  : y\n: *a\n")]
#[case::value_after_an_implicit_entry_ending_a_key("?\n  ? a\n  b: 1\n: v\n")]
#[case::value_after_a_nested_entry_with_an_empty_key("? ?\n  : v\n: w\n")]
#[case::comments_in_nested_explicit_keys("? # c\n  ? a # d\n  # e\n: v # f\n")]
fn nested_explicit_key_keeps_its_data_under_every_option_set(#[case] input: &str) {
    assert!(load(input).is_ok(), "the input must load: {input:?}");
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::dash_below_an_empty_explicit_key_in_a_value("x:\n  ?\n- a\n")]
#[case::dash_below_a_nested_explicit_key("?\n  ? a\n- b\n")]
#[case::colon_between_the_columns_of_nested_explicit_keys("?\n  ? a\n : v\n")]
fn nested_explicit_key_text_the_parser_rejects_is_returned_unchanged(#[case] input: &str) {
    assert!(load(input).is_err());
    assert_eq!(format_default(input), input);
}

// ---- the `---` line ---------------------------------------------------------

#[rstest]
#[case::flow_sequence("--- [alpha, bravo]\n", "---\n[alpha, bravo]\n")]
#[case::flow_mapping("--- {a: 1}\n", "---\n{ a: 1 }\n")]
#[case::alias("--- *a\n", "---\n*a\n")]
#[case::anchored_flow_sequence("--- &x [a]\n", "---\n&x [a]\n")]
#[case::flow_sequence_over_two_lines("--- [a,\n  b]\n", "---\n[a, b]\n")]
#[case::comment_after("--- [a] # c\n", "---\n[a]  # c\n")]
#[case::comment_on_the_next_line("--- [a]\n# c\n", "---\n[a]\n# c\n")]
#[case::alias_with_comments("--- *a # c\n# d\n", "---\n*a  # c\n# d\n")]
fn flow_node_on_the_marker_line_formats_as_the_root_node(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(format_default(input), expected);
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::collection_with_a_comment_inside("--- [ # c\n 1]\n", &["# c"])]
#[case::mapping_with_a_comment_inside("--- { # c\n a: 1}\n", &["# c"])]
#[case::comments_around("# p\n--- [a] # c\n# d\n", &["# p", "# c", "# d"])]
fn flow_node_on_the_marker_line_keeps_its_comments(#[case] input: &str, #[case] comments: &[&str]) {
    for (label, options) in option_sets() {
        let output = format_yaml(input, &options);
        for comment in comments {
            assert_eq!(
                output.matches(comment).count(),
                1,
                "{label}: comment {comment:?} in output {output:?}"
            );
        }
        assert_data_kept(input, &options, label);
    }
}

#[rstest]
#[case::continuation_line("--- x\n  y\n")]
#[case::continuation_at_column_zero("--- x\ny\n")]
#[case::percent_continuation("--- x\n%y\n")]
#[case::indicator_continuation("--- x\n- y\n")]
#[case::blank_line_between("--- x\n\n  y\n")]
#[case::comment_after_the_first_line("--- x # c\n")]
#[case::comment_after_a_continuation("--- x\n  y # c\n")]
fn plain_scalar_on_the_marker_line_keeps_its_data(#[case] input: &str) {
    let output = format_default(input);
    assert_eq!(format_default(&output), output, "not stable: {output:?}");
    assert_data_kept_under_every_option_set(input);
}

#[rstest]
#[case::continuation_line("--- x\n  y\n", "---\nx y\n")]
#[case::continuation_at_column_zero("--- x\ny\n", "---\nx y\n")]
#[case::percent_continuation("--- x\n%y\n", "---\nx %y\n")]
#[case::indicator_continuation("--- x\n- y\n", "---\nx - y\n")]
#[case::comment_after("--- x # c\n", "---\nx  # c\n")]
fn plain_scalar_on_the_marker_line_formats_as_the_root_node(
    #[case] input: &str,
    #[case] expected: &str,
) {
    assert_eq!(format_default(input), expected);
}

#[rstest]
#[case::two_collections("--- [x] [y]\n")]
#[case::collection_as_a_key("--- [x]: 1\n")]
#[case::alias_then_text("--- *a b\n")]
#[case::alias_named_with_a_colon_then_text("--- *a: v\n")]
#[case::closing_bracket("--- ]\n")]
#[case::closing_brace("--- }\n")]
#[case::closing_bracket_then_text("--- ]x\n")]
#[case::sequence_entry("--- - a\n")]
#[case::sequence_entry_then_bracket("--- - [\n")]
#[case::bare_dash("--- -\n")]
#[case::explicit_key("--- ? a\n")]
#[case::bare_question_mark("--- ?\n")]
#[case::plain_key("--- x: 1\n")]
#[case::plain_then_a_key("--- x\ny: 1\n")]
#[case::plain_then_text_after_a_comment("--- v\n# d\nw\n")]
fn invalid_marker_line_is_returned_unchanged(#[case] input: &str) {
    assert!(load(input).is_err());
    assert_eq!(format_default(input), input);
}

// ---- plain scalars over blank lines and comments in flow collections ---------

#[rstest]
#[case::item("[a\n\n  b]\n")]
#[case::item_over_two_blank_lines("[a\n\n\n b]\n")]
#[case::chained_continuations("[a\n\n b\n c\n\n\n d]\n")]
#[case::mapping_key("{a\n\n b: 1}\n")]
#[case::pair_key("[a\n\n b: 1]\n")]
#[case::explicit_key("{? a\n\n b: c}\n")]
#[case::pair_value("[a: b\n\n c]\n")]
#[case::in_a_block_mapping("x: [a\n\n  b]\n")]
#[case::as_an_explicit_block_key("? [a\n\n  b]\n: v\n")]
#[case::item_before_an_item("[a \n\n b, c]\n")]
fn flow_plain_scalar_over_blank_lines_keeps_its_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

#[test]
fn flow_plain_scalar_over_a_blank_line_is_written_with_its_line_feed() {
    assert_eq!(format_default("[a\n\n  b]\n"), "[\"a\\nb\"]\n");
}

#[rstest]
#[case::anchor("[a\n &x b]\n")]
#[case::tag_over_a_blank_line("[a\n\n !t b]\n")]
#[case::double_quote("[a\n \"b\"]\n")]
#[case::single_quote_over_a_blank_line("[a\n\n 'b']\n")]
#[case::alias("[a\n *x]\n")]
#[case::dash_over_a_blank_line("[a\n\n - b]\n")]
#[case::pipe_over_a_blank_line("[a\n\n |b]\n")]
#[case::question_mark("[a\n ? b]\n")]
#[case::anchor_line_and_alias_line("[a\n &x b\n\n *y c]\n")]
#[case::mapping_key_with_an_anchor("{a\n &x b: 1}\n")]
#[case::mapping_key_over_three_lines("{a\n b\n &x c: d}\n")]
#[case::mapping_key_without_a_value("{a\n b}\n")]
#[case::mapping_key_continued_by_a_colon("{a\n :x}\n")]
#[case::pair_key_with_a_double_quote("[a\n \"b\": 1]\n")]
#[case::pair_key_with_an_alias("[a\n *x: 1]\n")]
#[case::explicit_key_with_an_anchor("{? a\n &x b : c}\n")]
#[case::in_a_block_mapping("x: [a\n  &x b]\n")]
#[case::after_an_anchor("[&x a\n &y b]\n")]
fn flow_plain_scalar_continued_by_a_token_start_keeps_its_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

#[test]
fn flow_plain_scalar_continued_by_an_anchor_is_written_as_text() {
    let output = format_default("[a\n &x b]\n");
    let docs = load(&output).unwrap();
    let Node::Sequence { items, .. } = &docs[0].root else {
        panic!("expected a sequence: {output:?}");
    };
    assert_eq!(items.len(), 1, "{output:?}");
    assert!(
        matches!(&items[0], Node::Scalar { value, .. } if value == "a &x b"),
        "{output:?}"
    );
    assert_eq!(items[0].anchor(), None, "{output:?}");
}

#[rstest]
#[case::plain_item_after_a_comment("[a # c\n  b]\n")]
#[case::plain_key_after_a_comment("{a # c\n b: 1}\n")]
#[case::in_a_block_mapping("x: [a # c\n b]\n")]
#[case::after_a_continuation("[a\n\n  b #c\n  d]\n")]
#[case::before_an_anchor_line("[a # c\n  &x b]\n")]
fn plain_scalar_after_a_comment_in_a_flow_collection_is_returned_unchanged(#[case] input: &str) {
    assert!(load(input).is_err());
    assert_eq!(format_default(input), input);
}

#[rstest]
#[case::comment_before_a_comma("[a # c\n, b]\n")]
#[case::comment_after_a_comma("[a, # c\n b]\n")]
#[case::comment_after_a_continuation("[a\n\n b # c\n]\n")]
#[case::comment_after_a_continuation_with_a_property("[a\n &x b # c\n]\n")]
#[case::comment_before_the_colon_of_a_mapping_key("{a # c\n : 1}\n")]
fn comment_in_a_flow_collection_is_kept_once(#[case] input: &str) {
    for (label, options) in option_sets() {
        let output = format_yaml(input, &options);
        assert_eq!(output.matches("# c").count(), 1, "{label}: {output:?}");
        assert_data_kept(input, &options, label);
    }
}

// ---- keys that must stay distinct under duplicate-key removal ----------------

#[rstest]
#[case::null_key_and_empty_string_key(": a\n\"\": b\n")]
#[case::empty_string_key_and_null_key("\"\": a\n: b\n")]
#[case::int_key_and_string_key("1: a\n\"1\": b\n")]
#[case::bool_key_and_string_key("true: a\n\"true\": b\n")]
#[case::tagged_key_and_plain_key("!t a: 1\na: 2\n")]
fn duplicate_key_removal_keeps_keys_of_another_type(#[case] input: &str) {
    let options = options(|o| o.format_remove_duplicate_keys = true);
    assert_eq!(
        data_signature(&format_yaml(input, &options)),
        data_signature(input)
    );
}

#[test]
fn duplicate_key_removal_still_removes_the_same_key_in_another_style() {
    let options = options(|o| o.format_remove_duplicate_keys = true);
    assert_eq!(format_yaml("a: 1\n\"a\": 2\n'a': 3\n", &options), "a: 3\n");
}

// ---- values that single quotes would change ----------------------------------

#[rstest]
#[case::line_break("k: \"a\\nb\"\n")]
#[case::tab("k: \"\\ta\"\n")]
#[case::backslash("k: 'a\\b'\n")]
#[case::leading_space("k: \" lead\"\n")]
#[case::trailing_space("k: \"trail \"\n")]
#[case::spaces_only("k: \"  \"\n")]
#[case::line_separator("k: \"x\\u2028y\"\n")]
#[case::next_line("k: \"x\\Ny\"\n")]
#[case::in_a_flow_sequence("[\"a\\nb\", ' x']\n")]
#[case::as_a_key("\"a\\nb\": v\n")]
fn single_quote_option_never_changes_a_value_that_needs_escapes(#[case] input: &str) {
    let options = options(|o| o.single_quote = true);
    assert_data_kept(input, &options, "single quote");
}

#[test]
fn escaped_control_character_does_not_load_and_stays_as_it_is() {
    // The parser rejects the escape (U+0001 is not printable), so there is no
    // data to keep and the formatter returns the text unchanged.
    let input = "k: \"\\x01\"\n";
    assert!(load(input).is_err());
    for (label, options) in option_sets() {
        let expected = if options.line_ending == LineEnding::Crlf {
            input.replace('\n', "\r\n")
        } else {
            input.to_string()
        };
        assert_eq!(format_yaml(&expected, &options), expected, "{label}");
    }
}

// ---- tags, directives, anchors and aliases ------------------------------------

#[rstest]
#[case::tag_handle_from_a_directive("%TAG !e! tag:example.com,2000:\n---\n!e!x a\n")]
#[case::verbatim_tags("a: !<tag:yaml.org,2002:str> 5\nb: !<!foo> x\n")]
#[case::binary_and_timestamp("a: !!binary |\n  R0lGODlh\nb: !!timestamp 2001-12-14\n")]
#[case::merge_key_with_aliases("a: &x {b: 1}\nc:\n  <<: *x\nd:\n  <<: [*x]\n")]
#[case::anchored_flow_collection_as_a_key("&a [x]: v\nb: *a\n")]
#[case::anchor_alias_pair_in_a_flow_mapping("{&a a: &b b, *a : *b}\n")]
#[case::alias_in_a_second_document("a: &x 1\nb: [*x, *x]\n---\nc: &y [1]\nd: *y\n")]
#[case::anchored_flow_mapping_as_the_root("&a {b: 1}\n")]
#[case::tag_and_anchor_on_a_flow_mapping_after_the_marker("--- !t &a {a: 1}\n")]
#[case::alias_named_with_a_colon_on_the_marker_line("--- *a:\n")]
#[case::alias_named_with_a_colon_inside_on_the_marker_line("--- *a:b\n")]
fn tags_directives_anchors_and_aliases_keep_their_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

#[test]
fn flow_nodes_on_the_marker_lines_of_several_documents_keep_their_data() {
    let input = "--- [a]\n--- *a\n...\n--- {b: 1}\n";
    assert_eq!(
        format_default(input),
        "---\n[a]\n---\n*a\n...\n---\n{ b: 1 }\n"
    );
    assert_data_kept_under_every_option_set(input);
}

// ---- strings that start with an indicator ------------------------------------

#[rstest]
#[case::comma(",")]
#[case::comma_then_text(",a")]
#[case::closing_bracket("]")]
#[case::closing_bracket_then_text("]x")]
#[case::closing_brace("}")]
#[case::closing_brace_then_text("}x")]
#[case::opening_bracket("[x")]
#[case::opening_brace("{x")]
#[case::at_sign("@x")]
#[case::backtick("`x")]
#[case::percent("%x")]
#[case::tag("!x")]
#[case::anchor("&x")]
#[case::alias("*x")]
#[case::literal("|x")]
#[case::folded(">x")]
#[case::sequence_entry("- x")]
#[case::explicit_key("? x")]
#[case::colon_then_text(":x")]
#[case::single_quote("'x")]
#[case::double_quote("\"x")]
#[case::hash("#")]
#[case::leading_space(" x")]
fn string_starting_with_an_indicator_stays_quoted(#[case] value: &str) {
    let quoted = format!("{value:?}");
    for input in [
        format!("k: {quoted}\n"),
        format!("{quoted}: v\n"),
        format!("- {quoted}\n"),
        format!("[{quoted}]\n"),
        format!("{{{quoted}: 1}}\n"),
        format!("--- {quoted}\n"),
    ] {
        assert_data_kept_under_every_option_set(&input);
    }
}

// ---- strings that read like a document marker ---------------------------------

#[rstest]
#[case::root_after_the_marker_dots("--- ... [[\n")]
#[case::root_after_the_marker_dashes("--- --- x\n")]
#[case::root_dashes_alone("--- ---\n")]
#[case::root_dots_then_tab("--- ...\tx\n")]
#[case::quoted_root("--- \"... x\"\n")]
#[case::quoted_key_with_dots("\"... x\": 1\n")]
#[case::quoted_key_with_dashes("\"--- x\": 1\n")]
#[case::quoted_key_with_a_tab("\"---\\tx\": 1\n")]
#[case::quoted_item("- \"... x\"\n")]
#[case::quoted_value("a: \"... x\"\n")]
#[case::marker_without_a_gap_is_plain("a: ...x\n")]
fn string_that_reads_like_a_document_marker_keeps_its_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
}

// ---- text the parser rejects --------------------------------------------------

// A plain scalar cannot start with these indicators, and no line may start
// with a non-printable character, so the parser reports an error and the
// formatter returns the text unchanged rather than drop it.
#[rstest]
#[case::comma("--- ,\n")]
#[case::comma_then_text("--- ,x\n")]
#[case::percent("--- %a\n")]
#[case::at_sign("--- @a\n")]
#[case::backtick("--- `a\n")]
#[case::comma_with_a_comment("--- , # c\n")]
#[case::comma_as_a_mapping_value("v: ,\n")]
#[case::at_sign_as_a_sequence_item("- @a\n")]
#[case::backtick_as_a_key("`a: b\n")]
#[case::percent_below_the_marker("---\n%a\nb: 1\n")]
#[case::percent_only_below_the_marker("---\n%a\n")]
#[case::percent_then_key_value("---\n%a: b\n")]
#[case::percent_below_a_tag("---\n!t\n%a\n")]
#[case::percent_below_a_key("a:\n%b\n")]
#[case::percent_after_an_entry("a: 1\n%b\n")]
#[case::percent_before_a_second_document("---\n%a\n---\nx\n")]
#[case::percent_below_a_marker_comment("--- # c\n%a\n")]
#[case::bel_indented_at_the_root("  \x07zz\n")]
#[case::del_indented_below_a_key("a:\n  \x7fzz\n")]
#[case::bom_indented_at_the_root("  \u{feff}zz\n")]
#[case::bom_after_a_space_and_a_tab("---\n \t\u{feff}zz\n")]
fn text_the_parser_rejects_is_returned_unchanged(#[case] input: &str) {
    assert!(load(input).is_err());
    for (label, options) in option_sets() {
        let input = if options.line_ending == LineEnding::Crlf {
            input.replace('\n', "\r\n")
        } else {
            input.to_string()
        };
        assert_eq!(format_yaml(&input, &options), input, "{label}");
    }
}

// ---- keys and nesting at the parser's limits --------------------------------

#[test]
fn flow_key_longer_than_an_implicit_key_may_be_stays_a_key() {
    let item = "x".repeat(1_100);
    let input = format!("? [{item}]\n: v\n");
    assert_data_kept_under_every_option_set(&input);
    let input = format!("x: 1\n[{item}]: v\n");
    assert_data_kept_under_every_option_set(&input);
}

#[rstest]
#[case::mapping_key_holding_an_explicit_entry("? [{item}]: x\n")]
#[case::sequence_key_holding_an_explicit_entry("? - [{item}]: x\n: v\n")]
#[case::nested_mapping_key("? a:\n    [{item}]: c\n: d\n")]
fn key_that_holds_an_explicit_entry_with_a_flow_key_longer_than_an_implicit_key_may_be_loads(
    #[case] template: &str,
) {
    let item = "x".repeat(1_100);
    assert_data_kept_under_every_option_set(&template.replace("{item}", &item));
}

#[rstest]
#[case::anchors("{ ? {&BBB a: 1, &CCC b: 2}\n: v }\n", 'B', 'C')]
#[case::tags("{ ? {!<tag:BBB> a: 1, !<tag:CCC> b: 2}\n: v }\n", 'B', 'C')]
#[case::anchor_and_tag_on_one_key("{ ? [&BBB !<tag:CCC> a, b]\n: v }\n", 'B', 'C')]
fn key_whose_properties_make_it_longer_than_an_implicit_key_may_be_loads_at_any_width(
    #[case] template: &str,
    #[case] first: char,
    #[case] second: char,
) {
    let input = template
        .replace("BBB", &first.to_string().repeat(600))
        .replace("CCC", &second.to_string().repeat(600));
    for print_width in [80, 5_000, 100_000] {
        let options = options(|o| o.print_width = print_width);
        assert_data_kept(&input, &options, &format!("width {print_width}"));
    }
}

#[test]
fn flow_key_that_breaks_over_lines_stays_a_key() {
    let items = (0..40)
        .map(|n| format!("item{n}"))
        .collect::<Vec<_>>()
        .join(", ");
    for input in [
        format!("? [{items}]: x\n"),
        format!("? {{a: 1, b: [{items}]}}\n: v\n"),
    ] {
        assert_data_kept_under_every_option_set(&input);
    }
}

/// The stack of a test thread and of a tokio worker.
const TEST_THREAD_STACK: usize = 2 << 20;

/// Runs `f` on a thread with a stack of `bytes`.  A stack overflow aborts the
/// process, so a test that overflows takes down every test of the file.
#[expect(clippy::expect_used, reason = "test helper")]
fn on_stack(bytes: usize, f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(bytes)
        .spawn(f)
        .expect("the thread starts")
        .join()
        .expect("the formatter handles the nesting");
}

/// Runs `f` on a thread with a large stack: the formatter recurses once per
/// level of nesting, and a debug build of it needs more than the 2 MiB of a
/// test thread at the deepest nesting the parser accepts.
fn on_large_stack(f: impl FnOnce() + Send + 'static) {
    on_stack(64 << 20, f);
}

/// `levels` mappings, each the explicit key of the next, and a sequence in the
/// innermost key, one `?` per line and each line indented below the last.
fn question_mark_lines(levels: usize) -> String {
    (0..=levels)
        .flat_map(|depth| {
            let line = if depth < levels { "?\n" } else { "- x\n" };
            ["  ".repeat(depth), line.to_string()]
        })
        .collect()
}

/// `levels` mappings, each the explicit key of the next.
fn mappings_in_mapping_keys(levels: usize) -> String {
    format!("{}a\n", "? ".repeat(levels))
}

/// A mapping whose explicit key is a sequence, the first item of which is a
/// sequence, and so on: `levels` collections in all.
fn sequences_in_a_sequence_key(levels: usize) -> String {
    format!("? {}a\n", "- ".repeat(levels - 1))
}

/// An explicit key that is a sequence whose first item is a mapping with an
/// explicit key, and so on: `levels` collections in all.
fn mappings_and_sequences_in_turn(levels: usize) -> String {
    format!("{}a\n", "? - ".repeat(levels / 2))
}

#[test]
fn flow_sequence_on_the_marker_line_nested_as_deep_as_the_parser_accepts() {
    on_large_stack(|| {
        let levels = rlsp_yaml_parser::MAX_COLLECTION_DEPTH - 1;
        let input = format!("--- {}{}\n", "[".repeat(levels), "]".repeat(levels));
        assert_deep_input_is_formatted_under_every_option_set(
            &input,
            rlsp_yaml_parser::MAX_COLLECTION_DEPTH,
        );
        // The parser's own cap holds whatever the limit of the formatter.
        let too_deep = format!("--- {}{}\n", "[".repeat(levels + 2), "]".repeat(levels + 2));
        let unlimited = YamlFormatOptions {
            max_nesting_depth: usize::MAX,
            ..YamlFormatOptions::default()
        };
        assert_eq!(format_yaml(&too_deep, &unlimited), too_deep);
    });
}

#[test]
fn collection_keys_nested_as_deep_as_the_parser_accepts_keep_their_data() {
    on_large_stack(|| {
        let input = question_mark_lines(400);
        assert_deep_input_is_formatted_under_every_option_set(
            &input,
            rlsp_yaml_parser::MAX_COLLECTION_DEPTH,
        );
    });
}

#[rstest]
#[case::mappings_in_mapping_keys(mappings_in_mapping_keys)]
#[case::sequences_in_a_sequence_key(sequences_in_a_sequence_key)]
#[case::mappings_and_sequences_in_turn(mappings_and_sequences_in_turn)]
fn explicit_keys_nested_as_deep_as_the_parser_accepts_keep_their_data(
    #[case] build: fn(usize) -> String,
) {
    on_large_stack(move || {
        let input = build(rlsp_yaml_parser::MAX_COLLECTION_DEPTH - 1);
        assert_deep_input_is_formatted_under_every_option_set(
            &input,
            rlsp_yaml_parser::MAX_COLLECTION_DEPTH,
        );
    });
}

/// Mappings nested `levels` deep, one per line, and in the innermost
/// an entry with a comment the formatter moves (`k: 1 # c` becomes
/// `k: 1  # c`; the blank lines are paired by the text before the comment), a
/// blank line, and another entry.
fn mappings_with_a_blank_line_at_the_bottom(levels: usize) -> String {
    (0..levels)
        .map(|depth| format!("{}a:\n", "  ".repeat(depth)))
        .chain([format!("{0}k: 1 # c\n\n{0}j: 2\n", "  ".repeat(levels))])
        .collect()
}

fn deep_options(f: impl FnOnce(&mut YamlFormatOptions)) -> YamlFormatOptions {
    let mut deep = options(f);
    deep.max_nesting_depth = rlsp_yaml_parser::MAX_COLLECTION_DEPTH;
    deep
}

// The blank line is restored in the formatter's output, which the formatter
// loads again: with the limit of its options, not with the loader's default.
#[test]
fn blank_line_survives_in_a_text_nested_beyond_the_default_limit() {
    on_large_stack(|| {
        let input = mappings_with_a_blank_line_at_the_bottom(400);
        let output = format_yaml(&input, &deep_options(|_| {}));
        let bottom = format!("{0}k: 1  # c\n\n{0}j: 2\n", "  ".repeat(400));
        assert!(output.ends_with(&bottom), "the blank line is lost");
        assert_eq!(output.matches("\n\n").count(), 1);
    });
}

// The final line break is taken off only if the text without it loads to the
// same data: the formatter loads that text with the limit of its options.
#[test]
fn final_line_break_is_removed_in_a_text_nested_beyond_the_default_limit() {
    on_large_stack(|| {
        let input = mappings_with_a_blank_line_at_the_bottom(400);
        let output = format_yaml(&input, &deep_options(|o| o.insert_final_newline = false));
        assert!(output.ends_with("j: 2"), "the final line break stays");
        assert!(
            output.contains("k: 1  # c"),
            "the text came back unformatted"
        );
    });
}

// ---- nesting depth on the stack of a test thread -----------------------------

/// The nesting the formatter holds on the stack of a test thread, in levels of
/// collections: the default limit, which every option set takes.
const FORMATTED_DEPTH: usize = DEFAULT_MAX_NESTING_DEPTH;

/// One block mapping inside the other, each level on a line of its own.
fn block_mappings(levels: usize) -> String {
    (0..levels)
        .map(|depth| format!("{}a:\n", "  ".repeat(depth)))
        .chain([format!("{}x\n", "  ".repeat(levels))])
        .collect()
}

/// Like `block_mappings`, with an anchor, a tag and a comment on each level.
fn block_mappings_with_properties(levels: usize) -> String {
    (0..levels)
        .map(|depth| format!("{}a: &a{depth} !t # c\n", "  ".repeat(depth)))
        .chain([format!("{}x\n", "  ".repeat(levels))])
        .collect()
}

/// Block sequences and block mappings in turn, each sequence the value of the
/// mapping before it.
fn block_sequences_and_mappings_in_turn(levels: usize) -> String {
    (0..levels.div_ceil(2))
        .map(|depth| format!("{}- a:\n", "  ".repeat(depth)))
        .chain([format!("{}x\n", "  ".repeat(levels.div_ceil(2)))])
        .collect()
}

fn block_sequences(levels: usize) -> String {
    format!("{}x\n", "- ".repeat(levels))
}

fn flow_mappings(levels: usize) -> String {
    format!("{}x{}\n", "{a: ".repeat(levels), "}".repeat(levels))
}

fn flow_sequences(levels: usize) -> String {
    format!("{}x{}\n", "[".repeat(levels), "]".repeat(levels))
}

fn flow_sequences_on_the_marker_line(levels: usize) -> String {
    format!("--- {}x{}\n", "[".repeat(levels), "]".repeat(levels))
}

/// Mappings in mapping keys, one `?` per line and each line indented below the
/// last, with an anchor, a tag and a comment on each.
fn question_marks_with_properties(levels: usize) -> String {
    (0..levels)
        .map(|depth| format!("{}? &a{depth} !t # c\n", "  ".repeat(depth)))
        .chain([format!("{}a\n", "  ".repeat(levels))])
        .collect()
}

/// The sequence of `question_mark_lines` makes the last of the `levels`
/// collections.
fn question_mark_lines_ending_in_a_sequence(levels: usize) -> String {
    question_mark_lines(levels - 1)
}

// The shapes differ in the frames a level costs, so each has a case.
#[rstest]
#[case::block_mappings(block_mappings)]
#[case::block_mappings_with_properties(block_mappings_with_properties)]
#[case::block_sequences(block_sequences)]
#[case::block_sequences_and_mappings_in_turn(block_sequences_and_mappings_in_turn)]
#[case::flow_mappings(flow_mappings)]
#[case::flow_sequences(flow_sequences)]
#[case::flow_sequences_on_the_marker_line(flow_sequences_on_the_marker_line)]
#[case::mappings_in_mapping_keys(mappings_in_mapping_keys)]
#[case::sequences_in_a_sequence_key(sequences_in_a_sequence_key)]
#[case::mappings_and_sequences_in_turn(mappings_and_sequences_in_turn)]
#[case::question_marks_one_per_line(question_mark_lines_ending_in_a_sequence)]
#[case::question_marks_with_properties(question_marks_with_properties)]
fn nesting_at_the_formatted_depth_keeps_its_data_on_the_stack_of_a_test_thread(
    #[case] build: fn(usize) -> String,
) {
    let input = build(FORMATTED_DEPTH);
    // The input is as deep as the case says: the loader takes it at that limit
    // and no other.  Otherwise a miscounted case checks less than it claims.
    let loads_with_limit = |limit| {
        LoaderBuilder::new()
            .max_nesting_depth(limit)
            .build()
            .load(&input)
            .is_ok()
    };
    assert!(loads_with_limit(FORMATTED_DEPTH), "the input must load");
    assert!(
        !loads_with_limit(FORMATTED_DEPTH - 1),
        "the input is not as deep as the limit"
    );
    assert_eq!(
        YamlFormatOptions::default().max_nesting_depth,
        FORMATTED_DEPTH,
        "the option sets must run at the default limit"
    );
    on_stack(TEST_THREAD_STACK, move || {
        assert_data_kept_under_every_option_set(&input);
    });
}

// Both shapes change their text when they are formatted, so an output equal to
// the input means that the text was not loaded.
#[rstest]
#[case::flow_sequences(flow_sequences)]
#[case::flow_mappings(flow_mappings)]
fn default_options_format_the_default_depth_and_leave_one_level_deeper_alone(
    #[case] build: fn(usize) -> String,
) {
    on_stack(TEST_THREAD_STACK, move || {
        let options = YamlFormatOptions::default();
        let at_limit = build(DEFAULT_MAX_NESTING_DEPTH);
        assert_ne!(format_yaml(&at_limit, &options), at_limit);
        let over_limit = build(DEFAULT_MAX_NESTING_DEPTH + 1);
        assert_eq!(format_yaml(&over_limit, &options), over_limit);
    });
}

// ---- no panic on any text after the marker -----------------------------------

mod no_panic {
    use proptest::prelude::*;
    use proptest::test_runner::{Config, RngAlgorithm, TestRng, TestRunner};

    use super::*;

    #[test]
    fn any_text_after_the_marker_formats_without_a_panic() {
        // The alphabet holds what the marker line cares about: brackets, braces,
        // alias and comment indicators, quotes, colons, line ends, and
        // multi-byte characters.
        let alphabet = [
            "[", "]", "{", "}", "*", "#", "&", "!", ":", ",", "'", "\"", " ", "\t", "\n", "\r",
            "\r\n", "a", "ä", "日", "\u{feff}", "-", "?", "|", ">", "...", "---",
        ];
        let strategy =
            proptest::collection::vec(proptest::sample::select(alphabet.to_vec()), 0..24)
                .prop_map(|parts| parts.concat());
        let mut runner = TestRunner::new_with_rng(
            Config {
                cases: 3_000,
                ..Config::default()
            },
            TestRng::from_seed(RngAlgorithm::ChaCha, &[7; 32]),
        );
        runner
            .run(&strategy, |tail| {
                let input = format!("--- {tail}");
                for (_, options) in option_sets() {
                    let output = format_yaml(&input, &options);
                    if load(&input).is_ok() {
                        prop_assert!(load(&output).is_ok(), "input {input:?}, output {output:?}");
                    } else {
                        prop_assert_eq!(&output, &input);
                    }
                }
                Ok(())
            })
            .unwrap();
    }
}
