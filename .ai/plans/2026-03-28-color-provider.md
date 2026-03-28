# Color Provider

**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-28

## Goal

Add color detection and color picker integration to
rlsp-yaml via the `textDocument/documentColor` and
`textDocument/colorPresentation` LSP methods. This
enables inline color swatches and color picker UI in
editors for YAML files containing color values.

## Context

- Color values appear in YAML config files for themes,
  CI badges, UI settings, Kubernetes annotations, etc.
- The LSP spec defines two methods:
  - `documentColor`: returns color locations + RGBA values
  - `colorPresentation`: converts an RGBA color back to
    text representations the user can pick from
- vscode-json-languageservice implements this for JSON;
  rlsp-yaml does not yet
- No existing color detection infrastructure in the
  codebase — this is a new provider
- Server capabilities in `server.rs` need to advertise
  `ColorProviderCapability`

### Supported color formats

Detect these formats in YAML string values:
1. **Hex colors:** `#RGB`, `#RRGGBB`, `#RRGGBBAA`
2. **CSS named colors:** `red`, `blue`, `rebeccapurple`, etc. (148 standard CSS colors)
3. **RGB/RGBA functions:** `rgb(R, G, B)`, `rgba(R, G, B, A)`
4. **HSL/HSLA functions:** `hsl(H, S%, L%)`, `hsla(H, S%, L%, A)`

### Detection strategy

Scan YAML string values only (not keys) using regex
matching. Skip values inside comments. Only match whole
values or clearly delimited color tokens — avoid false
positives on arbitrary strings that happen to match
color patterns.

## Steps

- [ ] Add color detection module
- [ ] Implement `textDocument/documentColor`
- [ ] Implement `textDocument/colorPresentation`
- [ ] Register capability in server

## Tasks

### Task 1: Color detection and LSP integration

**Files:** new `color.rs`, `server.rs`, `lib.rs`

**Color detection (`color.rs`):**
- Define `ColorMatch` struct: range, RGBA color
- `find_colors(text: &str) -> Vec<ColorMatch>` scans
  text for color values
- Hex parser: `#` followed by 3, 6, or 8 hex digits,
  convert to RGBA floats (0.0-1.0)
- CSS named colors: static lookup table of the 148
  standard CSS color names → RGBA
- RGB/RGBA parser: `rgb(` or `rgba(` followed by
  comma-separated numbers and optional alpha
- HSL/HSLA parser: `hsl(` or `hsla(` with hue in
  degrees, saturation/lightness as percentages, optional
  alpha. Convert to RGB using standard algorithm.
- Only match within YAML values (not keys or comments)

**Color presentation (`color.rs`):**
- `color_presentations(color: Color) -> Vec<ColorPresentation>`
- Return hex, RGB, and HSL representations for a given
  color so the user can pick their preferred format

**Server integration (`server.rs`):**
- Add `ColorProviderCapability::Simple` to server
  capabilities
- Implement `document_color` handler: parse document,
  call `find_colors` on text, return `ColorInformation`
  items
- Implement `color_presentation` handler: call
  `color_presentations`, return results

- [ ] Create `color.rs` with hex color parsing
- [ ] Add CSS named color lookup table
- [ ] Add RGB/RGBA function parsing
- [ ] Add HSL/HSLA function parsing with HSL→RGB conversion
- [ ] Implement `color_presentations` for reverse conversion
- [ ] Add `document_color` and `color_presentation` handlers in `server.rs`
- [ ] Register `ColorProviderCapability` in server capabilities
- [ ] Unit tests for each color format
- [ ] Integration test with full YAML document
- [ ] Verify `cargo clippy` and `cargo test` pass

## Decisions

- **Single task:** This feature is self-contained — one
  new module plus server wiring. Splitting into multiple
  tasks would just add commit overhead for tightly coupled
  code.

- **Regex-based detection:** Matches the approach used by
  `document_links.rs` for URL detection. AST-based
  detection would miss colors in comments and add
  unnecessary coupling to the parser.

- **CSS named colors as static table:** Compile-time
  lookup is fast and avoids runtime parsing. 148 entries
  is small enough to embed directly.

- **No alpha in named colors:** CSS named colors don't
  have alpha variants. Alpha is only supported in hex
  (8-digit), RGBA, and HSLA formats.
