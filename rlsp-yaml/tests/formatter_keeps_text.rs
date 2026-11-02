// SPDX-License-Identifier: MIT
//
// The formatter deletes no text: when the parser reads a line, the data holds
// it, and `format_yaml` prints it again; otherwise the parser reports an error.
//
// A load that drops or misplaces text hides in the comparison of `load()` with
// `load()` that the other sweeps make.  This one adds text and looks for it.
// For every input (the cases of the yaml-test-suite, the real files in
// `tests/corpus/`, and the pinned inputs of `EXTRA_INPUTS`) and every position
// before a line (and after the last one) it inserts a line with a token that
// occurs nowhere else, in five forms (`zzN`, `%zzN`, `&zzN : v`, `!zzN : v`,
// `&zzN`) and at three columns (0, the indentation of the line, and that
// indentation plus 2).  A variant that does not load must come back unchanged
// from `format_yaml`.  For a variant that loads, the token is in the loaded
// data and in the output, and the output loads to the data of the variant.
//
// The one exception is a directive: `%zzN` before the `---` of the first
// document, or after a `...`, is a directive, which `load()` does not keep and
// the formatter drops (a separate plan).  The exception holds only for the
// token, and only for a variant whose data equals the data of its input.

#![expect(missing_docs, reason = "test code")]

mod inputs;
mod loaded_data;
mod suite_cases;

use std::collections::{BTreeMap, BTreeSet};
use std::fmt::Write as _;
use std::io::Write as _;
use std::panic::{AssertUnwindSafe, catch_unwind};
use std::sync::{Mutex, Once, OnceLock};
use std::thread::ThreadId;
use std::time::{Duration, Instant};

use inputs::{EXTRA_INPUTS, OptionSet, corpus_files, corpus_source, long_escaped_key, suite_files};
use rlsp_yaml::editing::formatter::format_yaml;
use rstest::rstest;

// ---- guards -----------------------------------------------------------------

/// How long one variant may take.  A variant takes milliseconds; one that does
/// not has sent the parser or the formatter into a loop.
const VARIANT_TIME_LIMIT: Duration = Duration::from_secs(30);

/// The address space of the test process.  A variant that makes the parser
/// allocate without end stops with an allocation failure instead of taking the
/// machine down.
#[cfg(all(target_os = "linux", target_env = "gnu"))]
const ADDRESS_SPACE_LIMIT: u64 = 4 << 30;

static RUNNING: Mutex<Vec<(ThreadId, Instant, String)>> = Mutex::new(Vec::new());

/// The variant a thread works on, listed while the guard lives.  The watchdog
/// reads the list; `eprintln!` would be captured and lost with the process.
struct Running(ThreadId);

impl Running {
    fn enter(what: String) -> Self {
        start_guards();
        let id = std::thread::current().id();
        if let Ok(mut running) = RUNNING.lock() {
            running.push((id, Instant::now(), what));
        }
        Self(id)
    }
}

impl Drop for Running {
    fn drop(&mut self) {
        if let Ok(mut running) = RUNNING.lock() {
            running.retain(|(id, _, _)| *id != self.0);
        }
    }
}

fn start_guards() {
    static STARTED: Once = Once::new();
    STARTED.call_once(|| {
        #[cfg(all(target_os = "linux", target_env = "gnu"))]
        {
            use rustix::process::{Resource, Rlimit, setrlimit};
            // A failure leaves the process without the limit, which only
            // loses the protection.
            let _ = setrlimit(
                Resource::As,
                Rlimit {
                    current: Some(ADDRESS_SPACE_LIMIT),
                    maximum: Some(ADDRESS_SPACE_LIMIT),
                },
            );
        }
        std::thread::spawn(|| {
            loop {
                std::thread::sleep(Duration::from_secs(1));
                let stuck = RUNNING.lock().ok().and_then(|running| {
                    running
                        .iter()
                        .find(|(_, since, _)| since.elapsed() > VARIANT_TIME_LIMIT)
                        .map(|(_, _, what)| what.clone())
                });
                if let Some(what) = stuck {
                    let _ = writeln!(
                        std::io::stderr(),
                        "a variant takes longer than {VARIANT_TIME_LIMIT:?}: {what}"
                    );
                    std::process::exit(1);
                }
            }
        });
    });
}

// ---- variants ---------------------------------------------------------------

/// What a variant inserts.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Form {
    /// `zzN`
    Scalar,
    /// `%zzN`
    Directive,
    /// `&zzN : v`
    AnchoredKey,
    /// `!zzN : v`
    TaggedKey,
    /// `&zzN`
    Anchor,
}

impl Form {
    const ALL: [Self; 5] = [
        Self::Scalar,
        Self::Directive,
        Self::AnchoredKey,
        Self::TaggedKey,
        Self::Anchor,
    ];

    fn line(self, token: &str) -> String {
        match self {
            Self::Scalar => token.to_string(),
            Self::Directive => format!("%{token}"),
            Self::AnchoredKey => format!("&{token} : v"),
            Self::TaggedKey => format!("!{token} : v"),
            Self::Anchor => format!("&{token}"),
        }
    }
}

/// Where a variant puts the inserted line, in relation to the line it goes
/// before.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
enum Column {
    Flush,
    Indent,
    IndentPlusTwo,
}

impl Column {
    /// The columns to try for a line with `indent` leading spaces, with the
    /// number of spaces each puts before the inserted line.  At indentation 0
    /// the first two coincide, so the second goes.
    fn spaces(indent: usize) -> Vec<(Self, usize)> {
        let mut columns = vec![(Self::Flush, 0)];
        if indent > 0 {
            columns.push((Self::Indent, indent));
        }
        columns.push((Self::IndentPlusTwo, indent + 2));
        columns
    }
}

/// A line inserted into an input.
#[derive(Clone, Copy, Debug)]
struct Variant {
    /// The line it goes before; the number of lines puts it at the end.
    position: usize,
    form: Form,
    column: Column,
    spaces: usize,
    /// Makes the token unique among all variants.
    number: usize,
}

impl Variant {
    /// Fixed width, so that no token contains another.
    fn token(&self) -> String {
        format!("zz{:08}", self.number)
    }

    fn text(&self, base: &str) -> String {
        let at: usize = base
            .split_inclusive('\n')
            .take(self.position)
            .map(str::len)
            .sum();
        let (before, after) = base.split_at(at);
        let separator = if before.is_empty() || before.ends_with('\n') {
            ""
        } else {
            "\n"
        };
        format!(
            "{before}{separator}{}{}\n{after}",
            " ".repeat(self.spaces),
            self.form.line(&self.token())
        )
    }

    /// The directive position of `%zzN` (see the header): column 0, the next
    /// line that carries content is the `---` of the first document, and what
    /// precedes is nothing or a `...`.
    fn is_directive_position(&self, base: &str) -> bool {
        if self.form != Form::Directive || self.spaces != 0 {
            return false;
        }
        let lines: Vec<&str> = base.lines().collect();
        let (before, after) = lines.split_at(self.position.min(lines.len()));
        after
            .iter()
            .copied()
            .find(|line| carries_content(line))
            .is_some_and(|line| is_marker(line, "---"))
            && before
                .iter()
                .rev()
                .copied()
                .find(|line| carries_content(line))
                .is_none_or(|line| is_marker(line, "..."))
    }
}

/// A line that is neither blank, nor a comment, nor a directive.
fn carries_content(line: &str) -> bool {
    let line = line.trim_matches(['\r', ' ', '\t']);
    !(line.is_empty() || line.starts_with(['#', '%']))
}

/// `line` is `marker` alone or followed by white space.
fn is_marker(line: &str, marker: &str) -> bool {
    line.strip_prefix(marker)
        .is_some_and(|rest| rest.is_empty() || rest.starts_with([' ', '\t', '\r']))
}

/// The indentation (spaces only) of the line a variant goes before; at the end
/// of the input, of the last line.
fn indent_at(base: &str, position: usize) -> usize {
    base.split_inclusive('\n')
        .nth(position)
        .or_else(|| base.split_inclusive('\n').next_back())
        .map_or(0, |line| line.len() - line.trim_start_matches(' ').len())
}

fn variants_of(text: &str, next_number: &mut usize) -> Vec<Variant> {
    let lines = text.split_inclusive('\n').count();
    (0..=lines)
        .flat_map(|position| {
            let indent = indent_at(text, position);
            Form::ALL.into_iter().flat_map(move |form| {
                Column::spaces(indent)
                    .into_iter()
                    .map(move |(column, spaces)| (position, form, column, spaces))
            })
        })
        .map(|(position, form, column, spaces)| {
            let number = *next_number;
            *next_number += 1;
            Variant {
                position,
                form,
                column,
                spaces,
                number,
            }
        })
        .collect()
}

// ---- the check --------------------------------------------------------------

#[derive(Debug, PartialEq, Eq)]
enum Verdict {
    /// The variant does not load, and the formatter returns it unchanged.
    NotLoading,
    /// A directive: the token is in neither the data nor the output, and the
    /// data are those of the input.
    Directive,
    /// The token is in the data and in the output, which loads to the same data.
    Kept,
    NotLoadingButChanged,
    DirectiveChangesData,
    TokenNotInData,
    TokenLostByFormatter,
    OutputDoesNotLoad,
    OutputDataDiffers,
    Panicked,
}

/// A variant to check, with what the load of it gave.
struct Case<'a> {
    text: &'a str,
    token: &'a str,
    /// The data of `text`; `None` when it does not load.
    data: Option<&'a str>,
    /// The data of the input `text` was made from.
    input_data: Option<&'a str>,
    directive_position: bool,
}

fn check_variant(
    case: &Case<'_>,
    format: &dyn Fn(&str) -> String,
    load: &dyn Fn(&str) -> Option<String>,
) -> Verdict {
    let output = format(case.text);
    let Some(data) = case.data else {
        return if output == case.text {
            Verdict::NotLoading
        } else {
            Verdict::NotLoadingButChanged
        };
    };
    let token_in_data = data.contains(case.token);
    if case.directive_position && !token_in_data {
        return if case.input_data == Some(data) {
            if load(&output).as_deref() == Some(data) {
                Verdict::Directive
            } else {
                Verdict::OutputDataDiffers
            }
        } else {
            Verdict::DirectiveChangesData
        };
    }
    if !token_in_data {
        return Verdict::TokenNotInData;
    }
    if !output.contains(case.token) {
        return Verdict::TokenLostByFormatter;
    }
    match load(&output) {
        None => Verdict::OutputDoesNotLoad,
        Some(loaded) if loaded == data => Verdict::Kept,
        Some(_) => Verdict::OutputDataDiffers,
    }
}

// ---- the inputs and the variants --------------------------------------------

/// An input and the variants made from it.
struct Base {
    source: String,
    text: String,
    /// The suite marks the input as invalid YAML.
    invalid: bool,
    /// `None` when the input does not load.
    data: Option<String>,
    entries: Vec<Entry>,
}

struct Entry {
    variant: Variant,
    /// `None` when the variant does not load.
    data: Option<String>,
    directive_position: bool,
}

struct Prepared {
    bases: Vec<Base>,
    /// The variants whose load panicked, with what the panic says.
    load_panics: Vec<String>,
}

/// The sources of the inputs with their text; whether the suite marks one as
/// invalid.
fn candidates() -> Vec<(String, String, bool)> {
    let mut candidates = Vec::new();
    for path in suite_files() {
        for case in suite_cases::load_cases_from_file(&path) {
            let source = format!("{}[{}] {}", case.file, case.index, case.name);
            candidates.push((source, case.yaml, case.fail));
        }
    }
    for path in corpus_files() {
        let text = std::fs::read_to_string(&path).unwrap_or_default();
        candidates.push((corpus_source(&path), text, false));
    }
    for (name, text) in EXTRA_INPUTS {
        candidates.push((format!("extra: {name}"), (*text).to_string(), false));
    }
    candidates.push((
        "extra: long escaped key".to_string(),
        long_escaped_key(),
        false,
    ));
    candidates
}

fn prepare() -> Prepared {
    let mut next_number = 0;
    let mut load_panics = Vec::new();
    let bases = candidates()
        .into_iter()
        .map(|(source, text, invalid)| {
            let data = loaded_data::data_signature(&text);
            let entries = variants_of(&text, &mut next_number)
                .into_iter()
                .map(|variant| {
                    let variant_text = variant.text(&text);
                    let _running = Running::enter(format!("loading {source} {variant:?}"));
                    let data = catch_unwind(|| loaded_data::data_signature(&variant_text))
                        .unwrap_or_else(|_| {
                            load_panics.push(format!("{source} {variant:?}: {variant_text:?}"));
                            None
                        });
                    let directive_position = variant.is_directive_position(&text);
                    Entry {
                        variant,
                        data,
                        directive_position,
                    }
                })
                .collect();
            Base {
                source,
                text,
                invalid,
                data,
                entries,
            }
        })
        .collect();
    Prepared { bases, load_panics }
}

fn prepared() -> &'static Prepared {
    static PREPARED: OnceLock<Prepared> = OnceLock::new();
    PREPARED.get_or_init(prepare)
}

// ---- the sweep --------------------------------------------------------------

/// The failures that a sweep shows in its message.
const MAX_SHOWN_FAILURES: usize = 10;

struct Failure {
    verdict: Verdict,
    form: Form,
    description: String,
}

#[derive(Default)]
struct Outcome {
    failures: Vec<Failure>,
    kept: usize,
    not_loading: usize,
    directives: usize,
}

fn run_sweep(prepared: &Prepared, format: &dyn Fn(&str) -> String) -> Outcome {
    let mut outcome = Outcome::default();
    for base in &prepared.bases {
        for entry in &base.entries {
            let variant = &entry.variant;
            let text = variant.text(&base.text);
            let token = variant.token();
            let _running = Running::enter(format!("{} {variant:?}: {text:?}", base.source));
            let case = Case {
                text: &text,
                token: &token,
                data: entry.data.as_deref(),
                input_data: base.data.as_deref(),
                directive_position: entry.directive_position,
            };
            let verdict = catch_unwind(AssertUnwindSafe(|| {
                check_variant(&case, format, &loaded_data::data_signature)
            }))
            .unwrap_or(Verdict::Panicked);
            match verdict {
                Verdict::Kept => outcome.kept += 1,
                Verdict::NotLoading => outcome.not_loading += 1,
                Verdict::Directive => outcome.directives += 1,
                Verdict::NotLoadingButChanged
                | Verdict::DirectiveChangesData
                | Verdict::TokenNotInData
                | Verdict::TokenLostByFormatter
                | Verdict::OutputDoesNotLoad
                | Verdict::OutputDataDiffers
                | Verdict::Panicked => {
                    let invalid = if base.invalid {
                        " (invalid in the suite)"
                    } else {
                        ""
                    };
                    let mut description = format!(
                        "{verdict:?}: {}{invalid} line {} {:?} {:?}\n    input:  {text:?}",
                        base.source,
                        variant.position + 1,
                        variant.form,
                        variant.column,
                    );
                    if verdict != Verdict::Panicked {
                        let _ = write!(description, "\n    output: {:?}", format(&text));
                    }
                    outcome.failures.push(Failure {
                        verdict,
                        form: variant.form,
                        description,
                    });
                }
            }
        }
    }
    outcome
}

fn failure_report(failures: &[Failure]) -> String {
    let mut per_form: BTreeMap<Form, usize> = BTreeMap::new();
    for failure in failures {
        *per_form.entry(failure.form).or_default() += 1;
    }
    let shown: Vec<&str> = failures
        .iter()
        .take(MAX_SHOWN_FAILURES)
        .map(|failure| failure.description.as_str())
        .collect();
    format!(
        "{} failures, per form {per_form:?}:\n{}",
        failures.len(),
        shown.join("\n")
    )
}

/// The option sets the sweep formats with.  `RemoveDuplicateKeys` and
/// `Combined` are left out on purpose: `format_remove_duplicate_keys` deletes
/// entries, and the comment sweep checks its data against `expected_data`.
const SWEPT: [OptionSet; 8] = [
    OptionSet::Default,
    OptionSet::EnforceBlockStyle,
    OptionSet::Narrow,
    OptionSet::IndentlessSequences,
    OptionSet::PreserveQuotes,
    OptionSet::SingleQuote,
    OptionSet::TabWidthFour,
    OptionSet::Yaml11,
];

#[rstest]
#[case::default(OptionSet::Default)]
#[case::enforce_block_style(OptionSet::EnforceBlockStyle)]
#[case::narrow(OptionSet::Narrow)]
#[case::indentless_sequences(OptionSet::IndentlessSequences)]
#[case::preserve_quotes(OptionSet::PreserveQuotes)]
#[case::single_quote(OptionSet::SingleQuote)]
#[case::tab_width_four(OptionSet::TabWidthFour)]
#[case::yaml_1_1(OptionSet::Yaml11)]
fn formatter_keeps_every_inserted_token(#[case] option_set: OptionSet) {
    let options = option_set.options();
    let outcome = run_sweep(prepared(), &|text| format_yaml(text, &options));
    println!(
        "{option_set:?}: kept {}, not loading {}, directives {}, failures {}",
        outcome.kept,
        outcome.not_loading,
        outcome.directives,
        outcome.failures.len()
    );
    assert!(
        outcome.failures.is_empty(),
        "{}",
        failure_report(&outcome.failures)
    );
}

#[test]
fn the_swept_option_sets_are_all_but_the_two_that_delete_entries() {
    let left_out: Vec<OptionSet> = OptionSet::ALL
        .into_iter()
        .filter(|set| !SWEPT.contains(set))
        .collect();
    assert_eq!(
        left_out,
        [OptionSet::RemoveDuplicateKeys, OptionSet::Combined]
    );
}

#[test]
fn no_variant_makes_the_loader_panic() {
    assert_eq!(prepared().load_panics, Vec::<String>::new());
}

// ---- the variants -----------------------------------------------------------

const fn variant(position: usize, form: Form, spaces: usize) -> Variant {
    Variant {
        position,
        form,
        column: Column::Flush,
        spaces,
        number: 1,
    }
}

#[rstest]
#[case::scalar(Form::Scalar, "zz00000001")]
#[case::directive(Form::Directive, "%zz00000001")]
#[case::anchored_key(Form::AnchoredKey, "&zz00000001 : v")]
#[case::tagged_key(Form::TaggedKey, "!zz00000001 : v")]
#[case::anchor(Form::Anchor, "&zz00000001")]
fn every_form_renders_its_line(#[case] form: Form, #[case] expected: &str) {
    assert_eq!(variant(0, form, 0).form.line("zz00000001"), expected);
    assert_eq!(variant(0, form, 0).token(), "zz00000001");
}

#[rstest]
#[case::flush_line(0, vec![(Column::Flush, 0), (Column::IndentPlusTwo, 2)])]
#[case::indented_line(4, vec![(Column::Flush, 0), (Column::Indent, 4), (Column::IndentPlusTwo, 6)])]
fn the_columns_of_a_line_do_not_repeat(
    #[case] indent: usize,
    #[case] expected: Vec<(Column, usize)>,
) {
    assert_eq!(Column::spaces(indent), expected);
}

#[rstest]
#[case::no_indentation("a\n", 0, 0)]
#[case::spaces("  a\n", 0, 2)]
#[case::a_tab_is_no_indentation("\ta\n", 0, 0)]
#[case::the_line_it_goes_before("a\n    b\n", 1, 4)]
#[case::the_last_line_at_the_end("a\n    b\n", 2, 4)]
#[case::empty_input("", 0, 0)]
fn the_indentation_is_that_of_the_line_the_variant_goes_before(
    #[case] input: &str,
    #[case] position: usize,
    #[case] expected: usize,
) {
    assert_eq!(indent_at(input, position), expected);
}

#[rstest]
#[case::empty_input("", 0, "  zz00000001\n")]
#[case::before_the_first_line("a\nb\n", 0, "  zz00000001\na\nb\n")]
#[case::in_the_middle("a\nb\n", 1, "a\n  zz00000001\nb\n")]
#[case::after_the_last_line("a\nb\n", 2, "a\nb\n  zz00000001\n")]
#[case::after_a_last_line_without_a_break("a\nb", 2, "a\nb\n  zz00000001\n")]
#[case::lines_ending_in_carriage_returns("a\r\nb\r\n", 1, "a\r\n  zz00000001\nb\r\n")]
fn a_variant_puts_its_line_before_the_given_line(
    #[case] input: &str,
    #[case] position: usize,
    #[case] expected: &str,
) {
    assert_eq!(variant(position, Form::Scalar, 2).text(input), expected);
}

#[rstest]
#[case::first_line("%zz00000001\n---\na\n", 0)]
#[case::after_a_comment("# c\n%zz00000001\n--- !!omap\n- a\n", 1)]
#[case::after_a_document_end("a\n...\n%zz00000001\n---\nb\n", 2)]
#[case::after_another_directive("%YAML 1.2\n%zz00000001\n---\na\n", 1)]
#[case::before_a_marker_with_a_value("%zz00000001\n--- a\n", 0)]
fn a_directive_goes_where_the_next_document_starts(#[case] text: &str, #[case] line: usize) {
    let base: String = text
        .split_inclusive('\n')
        .enumerate()
        .filter(|(index, _)| *index != line)
        .map(|(_, line)| line)
        .collect();
    assert!(variant(line, Form::Directive, 0).is_directive_position(&base));
}

#[rstest]
#[case::no_document_end_before("a\n%zz00000001\n---\nb\n", 1)]
#[case::no_marker_after("%zz00000001\na: 1\n", 0)]
#[case::a_block_scalar_before("--- |\n%zz00000001\n---\n", 1)]
fn a_directive_elsewhere_is_no_directive(#[case] text: &str, #[case] line: usize) {
    let base: String = text
        .split_inclusive('\n')
        .enumerate()
        .filter(|(index, _)| *index != line)
        .map(|(_, line)| line)
        .collect();
    assert!(!variant(line, Form::Directive, 0).is_directive_position(&base));
}

#[test]
fn only_a_directive_at_column_0_is_a_directive() {
    assert!(!variant(0, Form::Directive, 2).is_directive_position("---\na\n"));
    assert!(!variant(0, Form::Scalar, 0).is_directive_position("---\na\n"));
    assert!(!variant(0, Form::AnchoredKey, 0).is_directive_position("---\na\n"));
}

#[test]
fn every_token_has_one_width_and_belongs_to_one_variant() {
    let tokens: Vec<String> = prepared()
        .bases
        .iter()
        .flat_map(|base| base.entries.iter().map(|entry| entry.variant.token()))
        .collect();
    let widths: BTreeSet<usize> = tokens.iter().map(String::len).collect();
    assert_eq!(widths.len(), 1, "{widths:?}");
    assert_eq!(tokens.iter().collect::<BTreeSet<_>>().len(), tokens.len());
}

#[test]
fn no_input_holds_a_token_before_the_variant_adds_it() {
    for base in &prepared().bases {
        assert!(!base.text.contains("zz0"), "{}", base.source);
    }
}

// ---- the verdicts -----------------------------------------------------------

fn identity(text: &str) -> String {
    text.to_string()
}

/// A load that reads the text as its own data.
#[expect(
    clippy::unnecessary_wraps,
    reason = "has the signature of the load that `check_variant` takes"
)]
fn load_text(text: &str) -> Option<String> {
    Some(text.to_string())
}

const fn case<'a>(text: &'a str, data: Option<&'a str>) -> Case<'a> {
    Case {
        text,
        token: "zz00000001",
        data,
        input_data: Some("input"),
        directive_position: false,
    }
}

#[test]
fn a_formatter_that_changes_nothing_keeps_every_token() {
    let text = "a: zz00000001\n";
    let verdict = check_variant(&case(text, Some(text)), &identity, &load_text);
    assert_eq!(verdict, Verdict::Kept);
}

#[test]
fn a_variant_that_does_not_load_is_left_alone() {
    let verdict = check_variant(&case("a: [\n", None), &identity, &load_text);
    assert_eq!(verdict, Verdict::NotLoading);
}

#[test]
fn a_variant_that_does_not_load_must_come_back_unchanged() {
    let verdict = check_variant(&case("a: [\n", None), &|_| String::new(), &load_text);
    assert_eq!(verdict, Verdict::NotLoadingButChanged);
}

#[test]
fn a_token_missing_from_the_loaded_data_is_reported() {
    let verdict = check_variant(&case("a: zz00000001\n", Some("a")), &identity, &load_text);
    assert_eq!(verdict, Verdict::TokenNotInData);
}

#[test]
fn a_formatter_that_drops_the_token_is_reported() {
    let text = "a: zz00000001\n";
    let drop_token_line = |text: &str| text.replace("zz00000001", "");
    let verdict = check_variant(&case(text, Some(text)), &drop_token_line, &load_text);
    assert_eq!(verdict, Verdict::TokenLostByFormatter);
}

#[test]
fn a_formatter_that_drops_a_neighbour_is_reported_by_the_data() {
    let text = "a: zz00000001\nb: 1\n";
    let drop_neighbour = |text: &str| text.replace("b: 1\n", "");
    let verdict = check_variant(&case(text, Some(text)), &drop_neighbour, &load_text);
    assert_eq!(verdict, Verdict::OutputDataDiffers);
}

#[test]
fn an_output_that_does_not_load_is_reported() {
    let text = "a: zz00000001\n";
    let load_only_the_variant = |output: &str| (output == text).then(|| text.to_string());
    let verdict = check_variant(
        &case(text, Some(text)),
        &|output| format!("{output}["),
        &load_only_the_variant,
    );
    assert_eq!(verdict, Verdict::OutputDoesNotLoad);
}

#[test]
fn a_directive_may_lack_the_token_when_the_data_are_the_inputs() {
    let case = Case {
        text: "%zz00000001\n---\na\n",
        token: "zz00000001",
        data: Some("a"),
        input_data: Some("a"),
        directive_position: true,
    };
    let drop_directive = |text: &str| text.replace("%zz00000001\n", "");
    let load_scalar = |_: &str| Some("a".to_string());
    assert_eq!(
        check_variant(&case, &drop_directive, &load_scalar),
        Verdict::Directive
    );
}

#[test]
fn a_directive_that_changes_the_data_is_reported() {
    let case = Case {
        text: "%zz00000001\n---\na\n",
        token: "zz00000001",
        data: Some("a"),
        input_data: Some("b"),
        directive_position: true,
    };
    assert_eq!(
        check_variant(&case, &identity, &load_text),
        Verdict::DirectiveChangesData
    );
}

#[test]
fn a_directive_whose_output_has_other_data_is_reported() {
    let case = Case {
        text: "%zz00000001\n---\na\n",
        token: "zz00000001",
        data: Some("a"),
        input_data: Some("a"),
        directive_position: true,
    };
    let load_other = |_: &str| Some("b".to_string());
    assert_eq!(
        check_variant(&case, &identity, &load_other),
        Verdict::OutputDataDiffers
    );
}

// ---- inputs the plan names --------------------------------------------------

/// `format_yaml` and `load` on one variant, the way the sweep checks it.
fn verdict_of(text: &str, input: &str, directive_position: bool) -> Verdict {
    let data = loaded_data::data_signature(text);
    let input_data = loaded_data::data_signature(input);
    let case = Case {
        text,
        token: "zz00000001",
        data: data.as_deref(),
        input_data: input_data.as_deref(),
        directive_position,
    };
    check_variant(
        &case,
        &|text| format_yaml(text, &OptionSet::Default.options()),
        &loaded_data::data_signature,
    )
}

#[rstest]
#[case::marker_scalar_continuation("--- x\n  zz00000001\n", "--- x\n")]
#[case::colon_line_after_a_key("a:\n: zz00000001\n", "a:\n")]
#[case::bare_dash_before_a_comment("-\n# c\n  zz00000001\n", "-\n# c\n")]
#[case::flow_plain_scalar_over_a_blank_line("[a\n\n  zz00000001]\n", "[a]\n")]
#[case::properties_only_explicit_key_then_an_anchored_entry(
    "? &x\n&zz00000001 : v\n: v\n",
    "? &x\n: v\n"
)]
#[case::properties_only_explicit_key_then_a_tagged_entry(
    "? !!str\n!zz00000001 : v\n? &x\n",
    "? !!str\n? &x\n"
)]
#[case::properties_only_key_in_a_sequence("- ? &d\n  &zz00000001 : v\n", "- ? &d\n")]
fn the_formatter_keeps_the_token_in_an_input_the_plan_names(
    #[case] text: &str,
    #[case] input: &str,
) {
    assert_eq!(verdict_of(text, input, false), Verdict::Kept);
}

#[rstest]
#[case::percent_line_after_the_marker("---\n%zz00000001\nb: 1\n")]
#[case::percent_line_after_a_mapping("a: 1\n%zz00000001\n")]
#[case::second_tag_after_a_tag_and_an_anchor("a: !t &a\n  !zz00000001\n")]
#[case::second_tag_at_the_root("---\n!t &a\n!zz00000001\n")]
#[case::two_anchor_lines_on_a_mapping("&zz00000001\n&a4 !!map\nk: v\n")]
#[case::two_anchor_lines_after_the_marker("---\n&zz00000001\n&a4 !!map\nk: v\n")]
fn an_input_the_plan_names_is_an_error_and_comes_back_unchanged(#[case] text: &str) {
    assert_eq!(verdict_of(text, "", false), Verdict::NotLoading);
}

#[test]
fn a_directive_before_the_marker_is_exempt() {
    assert_eq!(
        verdict_of("%zz00000001\n---\na: 1\n", "---\na: 1\n", true),
        Verdict::Directive
    );
}

// ---- the sweep notices a formatter that loses text --------------------------

fn failures_by_verdict(outcome: &Outcome) -> BTreeMap<String, usize> {
    let mut by_verdict = BTreeMap::new();
    for failure in &outcome.failures {
        *by_verdict
            .entry(format!("{:?}", failure.verdict))
            .or_default() += 1;
    }
    by_verdict
}

fn failed_forms(outcome: &Outcome) -> BTreeSet<Form> {
    outcome
        .failures
        .iter()
        .map(|failure| failure.form)
        .collect()
}

#[test]
fn a_formatter_that_changes_nothing_passes_the_sweep() {
    let outcome = run_sweep(prepared(), &identity);
    assert!(
        outcome.failures.is_empty(),
        "{}",
        failure_report(&outcome.failures)
    );
    assert!(outcome.kept > 10_000, "{}", outcome.kept);
}

/// Removes the line that holds a token, the loss the sweep is for.
fn without_token_lines(text: &str) -> String {
    text.split_inclusive('\n')
        .filter(|line| !line.contains("zz0"))
        .collect()
}

#[test]
fn a_formatter_that_drops_the_token_line_fails_every_form() {
    let outcome = run_sweep(prepared(), &without_token_lines);
    assert_eq!(failed_forms(&outcome), Form::ALL.into_iter().collect());
    let by_verdict = failures_by_verdict(&outcome);
    assert!(
        by_verdict.contains_key("TokenLostByFormatter"),
        "{by_verdict:?}"
    );
}

/// Removes the line after the one that holds a token, which no token check sees.
fn without_the_line_after_a_token(text: &str) -> String {
    let mut drop_next = false;
    text.split_inclusive('\n')
        .filter(|line| {
            let dropped = std::mem::replace(&mut drop_next, line.contains("zz0"));
            !dropped
        })
        .collect()
}

#[test]
fn a_formatter_that_drops_the_line_after_the_token_is_found_through_the_data() {
    let outcome = run_sweep(prepared(), &without_the_line_after_a_token);
    let by_verdict = failures_by_verdict(&outcome);
    assert!(
        by_verdict.contains_key("OutputDataDiffers"),
        "{by_verdict:?}"
    );
    assert!(
        !by_verdict.contains_key("TokenLostByFormatter"),
        "{by_verdict:?}"
    );
}

#[test]
fn a_formatter_that_changes_an_input_that_does_not_load_fails_the_sweep() {
    let outcome = run_sweep(prepared(), &|text| format!("{text}\n"));
    let by_verdict = failures_by_verdict(&outcome);
    assert!(
        by_verdict.contains_key("NotLoadingButChanged"),
        "{by_verdict:?}"
    );
}

// ---- the sweep runs over what it should -------------------------------------

/// Floors that keep the sweep from emptying: a run in which few variants load
/// has stopped testing the formatter (the generator or the parser changed under
/// it).  Each sits well below what a run measures.
const MIN_LOADING_PERCENT: usize = 25;
const MIN_LOADING_PERCENT_PER_FORM: usize = 10;
const MIN_LOADING_PER_FORM_AND_COLUMN: usize = 100;
const MIN_INPUTS_WITH_LOADING_VARIANT_PERCENT: usize = 75;
const MAX_DIRECTIVE_PERCENT: usize = 20;

/// `(generated, loaded)` per form and column.
fn counts() -> BTreeMap<(Form, Column), (usize, usize)> {
    let mut counts: BTreeMap<(Form, Column), (usize, usize)> = BTreeMap::new();
    for base in &prepared().bases {
        for entry in &base.entries {
            let count = counts
                .entry((entry.variant.form, entry.variant.column))
                .or_default();
            count.0 += 1;
            count.1 += usize::from(entry.data.is_some());
        }
    }
    counts
}

fn percent(part: usize, whole: usize) -> usize {
    part * 100 / whole.max(1)
}

#[test]
fn most_inputs_and_forms_have_loading_variants() {
    let prepared = prepared();
    let counts = counts();
    let generated: usize = counts.values().map(|count| count.0).sum();
    let loaded: usize = counts.values().map(|count| count.1).sum();
    let with_loading_variant = prepared
        .bases
        .iter()
        .filter(|base| base.entries.iter().any(|entry| entry.data.is_some()))
        .count();
    println!(
        "inputs {}, with a loading variant {with_loading_variant}, variants {generated}, loading {loaded}",
        prepared.bases.len()
    );
    for ((form, column), (generated, loaded)) in &counts {
        println!("{form:?} {column:?}: {loaded} of {generated}");
    }
    assert!(percent(loaded, generated) >= MIN_LOADING_PERCENT);
    assert!(
        percent(with_loading_variant, prepared.bases.len())
            >= MIN_INPUTS_WITH_LOADING_VARIANT_PERCENT
    );
    for form in Form::ALL {
        let (generated, loaded) = counts
            .iter()
            .filter(|((counted, _), _)| *counted == form)
            .fold((0, 0), |sum, (_, count)| (sum.0 + count.0, sum.1 + count.1));
        assert!(
            percent(loaded, generated) >= MIN_LOADING_PERCENT_PER_FORM,
            "{form:?}: {loaded} of {generated}"
        );
    }
    for ((form, column), (_, loaded)) in &counts {
        assert!(
            *loaded >= MIN_LOADING_PER_FORM_AND_COLUMN,
            "{form:?} {column:?}: {loaded}"
        );
    }
}

#[test]
fn directives_are_a_small_part_of_the_loading_variants() {
    let entries: Vec<&Entry> = prepared()
        .bases
        .iter()
        .flat_map(|base| &base.entries)
        .filter(|entry| entry.data.is_some())
        .collect();
    let directives = entries
        .iter()
        .filter(|entry| entry.directive_position)
        .count();
    assert!(directives > 0, "no directive position left in the inputs");
    assert!(percent(directives, entries.len()) < MAX_DIRECTIVE_PERCENT);
}

#[test]
fn the_sweep_runs_over_the_suite_the_corpus_and_the_pinned_inputs() {
    let bases = &prepared().bases;
    let from = |prefix: &str| {
        bases
            .iter()
            .filter(|base| base.source.starts_with(prefix))
            .count()
    };
    assert!(from("corpus/") >= 5, "the corpus files");
    assert_eq!(from("extra: "), EXTRA_INPUTS.len() + 1);
    assert!(
        bases.len() - from("corpus/") - from("extra: ") > 300,
        "the suite cases"
    );
}

#[test]
fn every_pinned_input_has_a_loading_variant() {
    for base in prepared()
        .bases
        .iter()
        .filter(|base| base.source.starts_with("extra: "))
    {
        assert!(
            base.entries.iter().any(|entry| entry.data.is_some()),
            "{}",
            base.source
        );
    }
}
