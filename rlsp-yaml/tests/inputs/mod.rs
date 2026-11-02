// SPDX-License-Identifier: MIT
//
// The inputs that the formatter's sweeps run over besides the cases of the
// yaml-test-suite (see `suite_cases`): the real files in `tests/corpus/`, the
// pinned inputs of `EXTRA_INPUTS`, and the option sets the sweeps format with.
// Imported via `mod inputs;`; not part of `common`, which every test crate
// compiles.

use std::path::Path;

use rlsp_yaml::editing::formatter::YamlFormatOptions;
use rlsp_yaml::server::YamlVersion;

/// Inputs that the check holds besides the suite and the corpus: the positions
/// that once lost a comment or changed data.
pub const EXTRA_INPUTS: &[(&str, &str)] = &[
    (
        "tag and empty item with comments",
        "- !!str # c\n- # d\n- x\n",
    ),
    ("empty explicit keys and values", "?\n:\n?\n:\n"),
    (
        "comment lines below a bare dash before its value",
        "-\n# c\n  b\n- x\n",
    ),
    (
        "comment line below a bare dash before the next item",
        "-\n  # c\n- x\n",
    ),
    (
        "comment and blank lines below the last bare dash",
        "- a\n-\n  # c\n\n  # d\n",
    ),
    (
        "comment below a bare dash of a nested sequence",
        "a:\n  -\n  # c\n  - y\nz: 1\n",
    ),
    (
        "comment below a bare dash before a collection",
        "-\n  # c\n  k: v\n- -\n  # d\n- x\n",
    ),
    ("bare dash with a value below", "-\n  b\n- x\n"),
    (
        "comments at every position of a flow sequence",
        "[ # c1\n  a, # c2\n  # c3\n  [b, # c4\n    c], # c5\n  d # c6\n  # c7\n] # c8\n",
    ),
    (
        "comments at every position of a flow mapping",
        "{ # c1\n  a: 1, # c2\n  b # c3\n  : 2,\n  c: # c4\n    3,\n  # c5\n  d: [e, # c6\n    f]\n  # c7\n} # c8\n",
    ),
    ("anchor on an empty explicit key", "? &x\n: v\n"),
    ("anchor on an empty document", "&x\n---\ny\n"),
    ("tag on an empty document", "!t\n---\ny\n"),
    (
        "anchor on an empty document opened by a marker",
        "--- &x\n---\ny\n",
    ),
    (
        "anchor on an empty document before an end marker",
        "&x\n...\n",
    ),
    (
        "tag on a marker line before an end marker",
        "--- !t\n...\n---\ny\n",
    ),
    (
        "anchor on an empty document with a comment",
        "&x # c\n---\ny\n",
    ),
    ("comment without a line break at the end", "a: 1 # c"),
    ("only a comment without a line break", "# c"),
    ("item comment without a line break at the end", "- x # c"),
    ("tab before a tag on its own line", "-\n  \t!t a\n"),
    ("tab before an anchor on its own line", "k:\n  \t&x b\n"),
    ("tag alone in a flow sequence", "[ !!str ]\n"),
    ("anchor alone in a flow sequence", "[ &x ]\n"),
    ("tag alone as a flow mapping key", "{ !!str }\n"),
    ("tag as the last value of a flow mapping", "{ b: !!str }\n"),
    ("anchor as the last value of a flow mapping", "{ b: &x }\n"),
    ("anchor on the value of a single pair", "[ a: &x ]\n"),
    ("tag on an explicit key in a flow mapping", "{ ? !!str }\n"),
    (
        "anchor on a pair value next to an alias",
        "[{ b: &x }, *x]\n",
    ),
    ("tags before commas", "[a, !!str , b]\n"),
    (
        "tag alone as a value, then a key with a tag",
        ": !\n?\n! - :",
    ),
    (
        "tag alone as a value, then a key with an anchor",
        "a: !!str\n&x b: 1\n",
    ),
    ("flow key with a tagged mapping value", "? : ! {{}}"),
    (
        "scalar that ends in a colon in a flow mapping",
        "{: {!,!t v:: #\n}}",
    ),
    (
        "comments in a flow sequence value",
        "a: [\n  # c\n  1\n  ]\n",
    ),
    (
        "comments in a flow mapping value",
        "a: {\n  # c\n  b: 1\n  }\n",
    ),
    (
        "comments after items of a flow sequence",
        "[ # c\n a, # d\n b ] # e\n",
    ),
    ("properties on empty explicit keys", "? !!str\n? &x\n"),
    (
        "tag on an empty item of an indented sequence",
        "a:\n  - !!str\nb: 1\n",
    ),
    (
        "tag on an empty item of an indentless sequence",
        "a:\n- !!str\nb: 1\n",
    ),
    (
        "anchor on an empty item of an indentless sequence",
        "a:\n- &x\nb: *x\n",
    ),
    (
        "anchor and tag on an empty item of an indentless sequence",
        "a:\n- &x !!str\n# c\nb: *x\n",
    ),
    (
        "anchor in a removed duplicate entry",
        "a: &x 1\nb: *x\na: 2\n",
    ),
    (
        "anchor on a collection in a removed duplicate entry",
        "b: &z [1]\nb: *z\n",
    ),
    ("indented mapping in a sequence", "- a: 1\n  b: 2\n"),
    ("null value of an explicit key", "a: ?\n"),
    ("empty explicit key and value", "k:\n  ? \n  : \n"),
    ("quoted bool word", "a: \"y\"\nb: \"1_000\"\nc: 'on'\n"),
    (
        "quoted bool word as a key next to the plain one",
        "y: a\n\"y\": b\n",
    ),
    ("merge keys twice", "<<: *x\n<<: *y\n"),
    // Found by the generated inputs of the plan's fuzz probes.
    ("flow key with an empty key and a tagged value", "? ? : !\n"),
    (
        "flow key with an empty key and a value that starts with a colon",
        ":\n? : :c\n",
    ),
    (
        "property of the last item and a key with a property",
        ":\n - !\n! :\n",
    ),
    (
        "anchor and tag on empty items before a key",
        "a:\n- &d\n&a !u b: 1\n",
    ),
    ("path that starts with a tilde", "a: ~/bin\nb: ~x # c\n"),
    ("keep chomping at the end of the input", "a: |+\n  x"),
    (
        "tagged empty quoted scalars",
        "- !t '' # c\n- ! \"\"\n- !!null ''\n",
    ),
    (
        "block scalar key with an indentation indicator",
        "? !t >1  # d\n  t\n  u\n: *p\n",
    ),
    ("anchored empty key after an empty entry", ":\n? &a\n: #\n"),
    (
        "tagged empty value last in a flow mapping",
        "{ a: !!str \"\", b: !!str }\n",
    ),
    (
        "tagged empty block scalar with a header comment",
        "- !t |  # d\n\n- &a >-  # e\n",
    ),
    (
        "root block scalar with indicator and a comment below",
        "--- |1-\n# c\n",
    ),
    ("plain and quoted keys", "a: 1\n\"a\": 2\n'a': 3\n"),
    (
        "comment on a key with a removed duplicate",
        "# lead\na: 1 # one\nb: 2\na: 3 # three\n",
    ),
    (
        "properties before the colon of a pair in a flow sequence",
        "- [ !!str : a ]\n- [ !t : a ]\n- [ &x : a ]\n- [ !t &x : b ]\n- [ a, !t : b ]\n- [ !t : ]\n",
    ),
    (
        "explicit pairs in a flow sequence",
        "- [ ? !t : a ]\n- [ ? &x : a ]\n- [ ? !!str : !!str ]\n- [ ? a ]\n",
    ),
    (
        "question mark without a key in a flow sequence",
        "- [ ? ]\n- [ ? !!str ]\n- [ ? !!str , a ]\n- [ ? , a ]\n",
    ),
    (
        "property before the colon in a block value",
        "k: [ &x : a ]\n",
    ),
    (
        "question mark and collection key without a value in a flow mapping",
        "- { ? , a }\n- { [b] }\n- { [b] , c: d }\n",
    ),
    (
        "comments between the parts of a flow sequence pair",
        "[ ? # c1\n  !t # c2\n  : # c3\n  a # c4\n]\n",
    ),
];

/// The scalar key of 1000 U+2028 characters: escaping it makes it longer than
/// the implicit key limit.
pub fn long_escaped_key() -> String {
    format!("\"{}\": v # c\n", "\u{2028}".repeat(1000))
}

/// The files `tests/yaml-test-suite/src/*.yaml` at the workspace root, sorted by
/// name.
#[expect(clippy::unwrap_used, reason = "test code")]
pub fn suite_files() -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("../tests/yaml-test-suite/src");
    let mut files: Vec<_> = std::fs::read_dir(&dir)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| path.extension().is_some_and(|ext| ext == "yaml"))
        .collect();
    files.sort();
    files
}

pub fn corpus_source(path: &Path) -> String {
    format!(
        "corpus/{}",
        path.file_name().unwrap_or_default().to_string_lossy()
    )
}

pub fn corpus_files() -> Vec<std::path::PathBuf> {
    let dir = Path::new(env!("CARGO_MANIFEST_DIR")).join("tests/corpus");
    let mut files: Vec<_> = std::fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .map(|entry| entry.path())
        .filter(|path| {
            path.extension()
                .is_some_and(|ext| ext == "yml" || ext == "yaml")
        })
        .collect();
    files.sort();
    files
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum OptionSet {
    Default,
    EnforceBlockStyle,
    Narrow,
    IndentlessSequences,
    PreserveQuotes,
    SingleQuote,
    RemoveDuplicateKeys,
    TabWidthFour,
    Yaml11,
    /// Several options that change the layout at once.
    Combined,
}

impl OptionSet {
    pub fn options(self) -> YamlFormatOptions {
        let base = YamlFormatOptions::default;
        match self {
            Self::Default => base(),
            Self::EnforceBlockStyle => YamlFormatOptions {
                format_enforce_block_style: true,
                ..base()
            },
            Self::Narrow => YamlFormatOptions {
                print_width: 20,
                ..base()
            },
            Self::IndentlessSequences => YamlFormatOptions {
                format_indent_sequences: false,
                ..base()
            },
            Self::PreserveQuotes => YamlFormatOptions {
                preserve_quotes: true,
                ..base()
            },
            Self::SingleQuote => YamlFormatOptions {
                single_quote: true,
                ..base()
            },
            Self::RemoveDuplicateKeys => YamlFormatOptions {
                format_remove_duplicate_keys: true,
                ..base()
            },
            Self::TabWidthFour => YamlFormatOptions {
                tab_width: 4,
                ..base()
            },
            Self::Yaml11 => YamlFormatOptions {
                yaml_version: YamlVersion::V1_1,
                ..base()
            },
            Self::Combined => YamlFormatOptions {
                format_enforce_block_style: true,
                format_indent_sequences: false,
                print_width: 20,
                tab_width: 4,
                single_quote: true,
                format_remove_duplicate_keys: true,
                ..base()
            },
        }
    }

    pub const ALL: [Self; 10] = [
        Self::Default,
        Self::EnforceBlockStyle,
        Self::Narrow,
        Self::IndentlessSequences,
        Self::PreserveQuotes,
        Self::SingleQuote,
        Self::RemoveDuplicateKeys,
        Self::TabWidthFour,
        Self::Yaml11,
        Self::Combined,
    ];
}
