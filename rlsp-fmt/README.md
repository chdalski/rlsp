# rlsp-fmt

A reusable [Wadler-Lindig](https://homepages.inf.ed.ac.uk/wadler/papers/prettier/prettier.pdf) pretty-printing engine for the [rlsp](https://github.com/chdalski/rlsp) language server project.

## Overview

`rlsp-fmt` provides a document IR (`Doc`) and a width-aware printer. You describe your output as a tree of `Doc` nodes — text, groups, indentation, soft/hard line breaks — and the printer decides where to break lines to fit within a configurable width.

## Usage

```rust
use rlsp_fmt::{Doc, FormatOptions, format, group, concat, indent, line, text};

let doc = group(concat(vec![
    text("["),
    indent(concat(vec![line(), text("a"), text(","), line(), text("b")])),
    line(),
    text("]"),
]));

// Fits on one line at width 80
assert_eq!(format(&doc, &FormatOptions::default()), "[ a, b ]");

// Breaks across lines at width 10
let narrow = FormatOptions { print_width: 10, ..Default::default() };
assert_eq!(format(&doc, &narrow), "[\n  a,\n  b\n]");
```

## Doc Nodes

| Node | Description |
|---|---|
| `Text(String)` | Literal text (must not contain newlines) |
| `HardLine` | Mandatory line break regardless of mode |
| `Line` | Space in flat mode, newline + indent in break mode |
| `Indent(Doc)` | Increases indentation level for the child |
| `Group(Doc)` | Tries flat mode first; breaks if content exceeds width |
| `Concat(Vec<Doc>)` | Sequential composition of documents |
| `FlatAlt { flat, break_ }` | Different content depending on flat/break mode |

## Builder Functions

- `text(s)` — create a `Text` node
- `hard_line()` — create a `HardLine` node
- `line()` — create a `Line` node (soft break)
- `indent(doc)` — wrap in `Indent`
- `group(doc)` — wrap in `Group`
- `concat(docs)` — combine into `Concat`
- `flat_alt(flat, break_)` — mode-dependent content
- `join(separator, docs)` — intersperse a separator between documents

## FormatOptions

| Option | Default | Description |
|---|---|---|
| `print_width` | 80 | Maximum line width before breaking groups |
| `tab_width` | 2 | Spaces per indentation level (ignored when `use_tabs` is true) |
| `use_tabs` | false | Use tab characters instead of spaces |

## Documentation

- [Feature Log](docs/feature-log.md) — implemented capabilities and design decisions

## Building

```sh
cargo build -p rlsp-fmt
cargo test -p rlsp-fmt
```

## License

[MIT](../LICENSE) — Christoph Dalski

## AI Note

Every line of source in this crate was authored, reviewed, and committed by AI agents
working through a multi-agent pipeline (planning, implementation, independent review,
and test/security advisors for high-risk tasks). The human role is designing the
architecture, rules, and review process; agents execute them. Conformance against the
YAML Test Suite is a measured acceptance criterion — not an aspiration — and any change
touching parser behaviour or untrusted input passes through formal test and security
advisor review before being merged.
