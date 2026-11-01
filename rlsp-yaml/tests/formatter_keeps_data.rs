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
use rlsp_yaml_parser::load;
use rstest::rstest;

// ---- helpers ----------------------------------------------------------------

/// The data of `yaml` (see `loaded_data::data_signature`); `yaml` must load.
#[expect(clippy::expect_used, reason = "test helper")]
fn data_signature(yaml: &str) -> String {
    loaded_data::data_signature(yaml).expect("the input must load")
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
    let input = if options.line_ending == LineEnding::Crlf {
        input.replace('\n', "\r\n")
    } else {
        input.to_string()
    };
    let output = format_yaml(&input, options);
    let context = format!("{label}: input {input:?}, output {output:?}");
    assert!(load(&output).is_ok(), "the output does not load: {context}");
    assert_eq!(
        data_signature(&output),
        data_signature(&input),
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

fn format_default(input: &str) -> String {
    format_yaml(input, &YamlFormatOptions::default())
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
#[case::default(YamlFormatOptions::default(), "? { [a]: x }\n:\n")]
#[case::enforce_block_style(
    YamlFormatOptions { format_enforce_block_style: true, ..YamlFormatOptions::default() },
    "? { [a]: x }\n:\n"
)]
fn mapping_key_that_holds_an_explicit_entry_prints_as_a_flow_mapping(
    #[case] options: YamlFormatOptions,
    #[case] expected: &str,
) {
    let input = "? [a]: x\n";
    assert_eq!(format_yaml(input, &options), expected);
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
#[case::mapping_in_a_mapping_key("? a:\n    [b]: c\n: d\n")]
#[case::sequence_key_holding_an_explicit_entry("? - [b]: c\n: d\n")]
#[case::empty_item_in_a_key_turned_flow("? - [x]: !!str null\n  - 5\n  -\n:\n")]
#[case::several_entries("? [a]: x\n  [b]: y\n")]
#[case::as_a_mapping_value_key("k:\n  ? [a]: x\n")]
#[case::second_key_of_a_mapping("z: 1\n? [a]: x\n")]
#[case::block_scalar_inside("? a: 1\n  [b]: |\n    lit\n: v\n")]
#[case::comments_inside("? a: 1 # c1\n  [b]: x # c2\n: v # c3\n")]
fn collection_key_that_the_parser_cannot_read_in_block_form_keeps_its_data(#[case] input: &str) {
    assert_data_kept_under_every_option_set(input);
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

// ---- root scalars the parser takes from the `---` line ------------------------

// A plain scalar cannot start with these indicators, so the parser reports an
// error and the formatter returns the text unchanged.  Before, the parser took
// the text of a `---` line as a scalar, and the line below dropped it.
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
fn text_that_starts_with_a_reserved_indicator_is_returned_unchanged(#[case] input: &str) {
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

/// Runs `f` on a thread with a large stack: a debug build of the loader needs
/// more than the 2 MiB of a test thread at the deepest nesting the parser accepts.
#[expect(clippy::expect_used, reason = "test helper")]
fn on_large_stack(f: impl FnOnce() + Send + 'static) {
    std::thread::Builder::new()
        .stack_size(64 << 20)
        .spawn(f)
        .expect("the thread starts")
        .join()
        .expect("the formatter handles the deepest nesting the parser accepts");
}

#[test]
fn flow_sequence_on_the_marker_line_nested_as_deep_as_the_parser_accepts() {
    on_large_stack(|| {
        let levels = rlsp_yaml_parser::MAX_COLLECTION_DEPTH - 1;
        let input = format!("--- {}{}\n", "[".repeat(levels), "]".repeat(levels));
        assert_data_kept_under_every_option_set(&input);
        let too_deep = format!("--- {}{}\n", "[".repeat(levels + 2), "]".repeat(levels + 2));
        assert_eq!(format_default(&too_deep), too_deep);
    });
}

#[test]
fn collection_keys_nested_as_deep_as_the_parser_accepts_keep_their_data() {
    on_large_stack(|| {
        let levels = 400;
        let input: String = (0..=levels)
            .flat_map(|depth| {
                let line = if depth < levels { "?\n" } else { "- x\n" };
                ["  ".repeat(depth), line.to_string()]
            })
            .collect();
        assert_data_kept_under_every_option_set(&input);
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
