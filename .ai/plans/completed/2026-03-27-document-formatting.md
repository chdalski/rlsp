**Repository:** root
**Status:** Completed (2026-03-27)
**Created:** 2026-03-27

## Goal

Add full document formatting (`textDocument/formatting`)
to rlsp-yaml, built on a reusable pretty-printing engine
crate (`rlsp-fmt`) that can serve future JSON and TOML
language servers. The engine implements the Wadler-Lindig
algorithm; the YAML formatter walks saphyr's AST and
emits IR nodes that the engine renders with line-width
awareness.

## Context

- No existing formatting support in rlsp-yaml. Red Hat's
  server uses Prettier (JS). We build our own in Rust,
  on saphyr.
- The Wadler-Lindig algorithm is the standard approach
  used by Prettier, `pretty_yaml`, and most modern
  formatters. It separates document description (IR) from
  rendering (printer), using group-based flat/break
  decisions with lookahead.
- Saphyr does not expose comment positions in its AST.
  Comments must be extracted from raw text via line
  scanning and reattached to AST nodes by proximity.
- The workspace currently has one member (`rlsp-yaml`).
  Adding `rlsp-fmt` as a second workspace member is the
  natural integration point.
- Formatting options should follow Prettier conventions
  where applicable: `printWidth`, `tabWidth`, `useTabs`,
  `singleQuote`, `bracketSpacing`.

### Workspace layout after implementation

```text
/
├── Cargo.toml              # members = ["rlsp-fmt", "rlsp-yaml"]
├── rlsp-fmt/
│   ├── Cargo.toml          # no external dependencies
│   └── src/
│       ├── lib.rs          # public API: Doc, FormatOptions, format()
│       ├── ir.rs           # IR node enum
│       └── printer.rs      # Wadler-Lindig printer
├── rlsp-yaml/
│   ├── Cargo.toml          # + rlsp-fmt dependency
│   └── src/
│       ├── formatter.rs    # NEW: saphyr AST → IR → formatted text
│       └── ...
```

### IR node types (minimal set)

```rust
enum Doc {
    Text(String),          // literal content
    HardLine,              // mandatory line break
    Line,                  // soft break: space in flat, newline in break
    Indent(Box<Doc>),      // increase indent for child
    Group(Box<Doc>),       // flat/break decision boundary
    Concat(Vec<Doc>),      // sequential composition
    FlatAlt {              // different content in flat vs break modes
        flat: Box<Doc>,
        break_: Box<Doc>,
    },
}
```

### Key files

- `rlsp-fmt/src/ir.rs` — IR types
- `rlsp-fmt/src/printer.rs` — Wadler-Lindig printer
- `rlsp-yaml/src/formatter.rs` — YAML AST → IR builder
- `rlsp-yaml/src/server.rs` — LSP `textDocument/formatting`
  handler integration
- `rlsp-yaml/docs/configuration.md` — formatting settings

## Steps

- [x] Create `rlsp-fmt` crate with IR and printer (79c8d2c)
- [x] Add YAML formatter (AST → IR, no comments) (1e14890)
- [x] Add comment preservation (2afd833)
- [x] Wire into LSP `textDocument/formatting` (874ffa5)
- [x] Add formatting settings (874ffa5)
- [x] Write tests (79c8d2c, 1e14890, 2afd833)
- [x] Update documentation (8663af8)

## Tasks

### Task 1: Create `rlsp-fmt` crate — IR and printer

Create the `rlsp-fmt` crate as a new workspace member
with the Wadler-Lindig pretty printing engine.

Files: `rlsp-fmt/Cargo.toml`, `rlsp-fmt/src/lib.rs`,
`rlsp-fmt/src/ir.rs`, `rlsp-fmt/src/printer.rs`,
`Cargo.toml` (workspace)

- [ ] Create `rlsp-fmt/Cargo.toml` — no external deps,
      inherit workspace lints and package metadata
- [ ] Add `"rlsp-fmt"` to workspace members in root
      `Cargo.toml`
- [ ] Implement `Doc` enum in `ir.rs` with variants:
      `Text`, `HardLine`, `Line`, `Indent`, `Group`,
      `Concat`, `FlatAlt`
- [ ] Add builder functions: `text()`, `hard_line()`,
      `line()`, `indent()`, `group()`, `concat()`,
      `flat_alt()`, plus convenience helpers like
      `join()` (intersperse separator between docs)
- [ ] Implement `FormatOptions` struct: `print_width: usize`
      (default 80), `tab_width: usize` (default 2),
      `use_tabs: bool` (default false)
- [ ] Implement the Wadler-Lindig printer in `printer.rs`:
      - `format(doc: &Doc, options: &FormatOptions) -> String`
      - Internal state: current column, indent level, mode
        stack (flat/break)
      - `fits(doc, remaining_width) -> bool` lookahead
      - Flat mode: `Line` → space, `FlatAlt` → flat variant
      - Break mode: `Line` → newline + indent,
        `FlatAlt` → break variant
      - `HardLine` always breaks regardless of mode
      - `Group` checks `fits()` to decide flat vs break
- [ ] Re-export public API from `lib.rs`
- [ ] Unit tests for the printer:
      - Simple text renders as-is
      - Group that fits → flat (single line)
      - Group that doesn't fit → break (multi-line)
      - Nested groups with independent decisions
      - Indent increases indentation in break mode
      - HardLine forces break even in flat mode
      - FlatAlt uses correct variant per mode
      - join() helper intersperse
      - use_tabs produces tab characters
      - Various print_width settings

### Task 2: YAML formatter — basic structure

Add the YAML-specific formatter to `rlsp-yaml` that walks
saphyr's AST and produces `rlsp-fmt` IR. Start without
comment preservation — structure first.

Files: `rlsp-yaml/Cargo.toml`, `rlsp-yaml/src/lib.rs`,
`rlsp-yaml/src/formatter.rs`

- [ ] Add `rlsp-fmt = { path = "../rlsp-fmt" }` to
      `rlsp-yaml/Cargo.toml` dependencies
- [ ] Add `mod formatter;` to `lib.rs`
- [ ] Implement `pub fn format_yaml(text: &str,
      options: &YamlFormatOptions) -> Result<String, FormatError>`
- [ ] `YamlFormatOptions` struct: wraps `rlsp_fmt::FormatOptions`
      plus YAML-specific options: `single_quote: bool`
      (default false), `bracket_spacing: bool`
      (default true), `always_block: bool` (default false)
- [ ] AST walker that handles:
      - **Scalars:** plain, single-quoted, double-quoted,
        literal block (`|`), folded block (`>`)
      - **Mappings:** block style with `key: value` pairs,
        proper indentation of nested values
      - **Sequences:** block style with `- item` entries
      - **Flow collections:** `{a: 1, b: 2}` and
        `[1, 2, 3]` with group-based flat/break decisions
      - **Multi-document:** preserve `---` separators
      - **Anchors and aliases:** `&anchor` and `*alias`
      - **Tags:** `!include`, `!!str`, etc.
      - **Null/empty values:** preserve empty mappings and
        sequences
- [ ] Unit tests:
      - Simple key-value formatting
      - Nested mappings indent correctly
      - Sequences format with `- ` prefix
      - Flow collections break when exceeding print width
      - Multi-document with `---`
      - Anchors and aliases preserved
      - Quote style respected
      - Round-trip: format(format(x)) == format(x)
        (idempotency)

### Task 3: Comment preservation

Add comment extraction from raw text and reattachment
to the formatted output.

Files: `rlsp-yaml/src/formatter.rs`

- [ ] Implement comment scanner: extract all `#` comments
      with their line numbers and column positions from
      raw text
- [ ] Classify comments:
      - **Trailing:** on the same line as code
        (`key: value  # comment`)
      - **Leading:** on a line by itself before a node
        (`# comment\nkey: value`)
      - **Blank-line separated:** leading comment with
        blank line before it (section divider)
- [ ] Attach comments to AST nodes by proximity:
      - Trailing comments attach to the node on the same
        line
      - Leading comments attach to the next node below
      - Preserve blank lines between comment groups
- [ ] Emit comments as `Text` nodes in the IR at the
      correct positions, with proper indentation
- [ ] Unit tests:
      - Trailing comments preserved
      - Leading comments preserved
      - Blank lines between sections preserved
      - Comments at document start/end preserved
      - Inline comments on flow collections
      - Comments between sequence items
      - Idempotency with comments

### Task 4: LSP integration and settings

Wire the formatter into the LSP `textDocument/formatting`
handler and add configurable settings.

Files: `rlsp-yaml/src/server.rs`,
`rlsp-yaml/src/formatter.rs`

- [ ] Add `DocumentFormattingProvider` capability to
      `Backend::capabilities()`
- [ ] Implement `formatting()` method on the
      `LanguageServer` trait impl:
      - Retrieve document text from `document_store`
      - Extract `FormattingOptions` from the LSP request
        (tab_size, insert_spaces)
      - Merge with workspace formatting settings
      - Call `format_yaml()` and return `TextEdit` spanning
        the full document
      - Return empty vec on format error (don't crash)
- [ ] Add formatting settings to `Settings` struct:
      `format_print_width: Option<usize>`,
      `format_single_quote: Option<bool>`,
      `format_bracket_spacing: Option<bool>`
- [ ] LSP `FormattingOptions.tab_size` and `insert_spaces`
      take precedence over workspace settings (editor
      controls indentation)

### Task 5: Documentation

Update docs for the new formatting capability.

Files: `rlsp-yaml/docs/configuration.md`,
`rlsp-yaml/docs/feature-log.md`,
`CLAUDE.md` (project structure update)

- [ ] Add formatting settings to `configuration.md`
- [ ] Document formatting behavior and options
- [ ] Mark "Full Document Formatting" as `[completed]`
      in `feature-log.md`
- [ ] Update project structure in root `CLAUDE.md` to
      include `rlsp-fmt/` crate

## Decisions

- **Own engine vs dependency:** Build our own `rlsp-fmt`
  crate. Enables reuse across future LSPs (JSON, TOML),
  avoids pulling in a second YAML parser (`pretty_yaml`
  uses `yaml_parser`, not saphyr), and keeps the binary
  lean. The Wadler-Lindig algorithm is well-documented
  and ~500-800 lines.
- **Wadler-Lindig algorithm:** Industry standard used by
  Prettier, `pretty_yaml`, and most modern formatters.
  Linear time, optimal line-breaking decisions via
  group-based flat/break strategy.
- **Comment preservation via line scanning:** Saphyr
  doesn't expose comment positions. Extract comments
  from raw text by scanning for `#` outside strings,
  classify by position (trailing/leading), attach to
  nearest AST node. This is the same approach Prettier
  uses.
- **No range formatting yet:** Start with full-document
  formatting only. Range formatting (Task 4 in
  feature-log) can be added later by restricting the
  formatter to a sub-tree.
- **Settings follow Prettier conventions:** `printWidth`,
  `tabWidth`, `singleQuote`, `bracketSpacing` — users
  familiar with Prettier will recognize these.
- **LSP `tab_size`/`insert_spaces` override workspace
  settings:** The editor controls indentation; workspace
  settings control YAML-specific style choices.
