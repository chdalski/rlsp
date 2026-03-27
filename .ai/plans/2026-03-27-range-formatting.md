**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-27

## Goal

Add `textDocument/rangeFormatting` so users can format a
selected region of a YAML document instead of the entire
file. This complements the full-document formatting added
in the previous plan.

## Context

- Full-document formatting is already implemented
  (`server.rs:725`, `formatter.rs`).
- The LSP `textDocument/rangeFormatting` handler receives a
  `Range` parameter indicating which lines to format.
- Strategy: format the full document, then return a
  `TextEdit` covering only the requested range with the
  corresponding lines from the formatted output. This
  avoids partial-tree formatting complexity — the
  Wadler-Lindig printer needs the full document context
  to make correct line-breaking decisions.
- Key files: `server.rs` (handler + capability),
  `configuration.md`, `feature-log.md`.
- The existing `format_yaml` function and `YamlFormatOptions`
  are reused unchanged.

## Steps

- [ ] Add `rangeFormatting` handler and capability
- [ ] Write tests
- [ ] Update documentation

## Tasks

### Task 1: Range formatting handler

Add `textDocument/rangeFormatting` support to the LSP
server.

Files: `rlsp-yaml/src/server.rs`

- [ ] Add `document_range_formatting_provider:
      Some(OneOf::Left(true))` to `capabilities()`
- [ ] Implement `range_formatting` method on
      `LanguageServer` trait impl:
      1. Get document text from `document_store`
      2. Build `YamlFormatOptions` (same logic as
         `formatting` handler)
      3. Call `format_yaml` on the full document
      4. Extract lines from the formatted output that
         correspond to the requested range
      5. Return a `TextEdit` covering only the requested
         range with the extracted formatted lines
      6. Return `Ok(None)` if the range content is unchanged
- [ ] Add `DocumentRangeFormattingParams` to imports
- [ ] Unit/integration test: verify the capability is
      advertised and the handler produces a range-scoped edit

### Task 2: Documentation

Files: `rlsp-yaml/docs/configuration.md`,
`rlsp-yaml/docs/feature-log.md`

- [ ] Document range formatting in configuration.md
      (in the Formatting section — mention that range
      formatting uses the same settings as full-document)
- [ ] Mark "Range Formatting" as `[completed]` in
      feature-log.md

## Decisions

- **Full-document format + range extract:** Formatting a
  sub-tree in isolation would produce different
  line-breaking decisions than formatting the full document
  (the printer needs surrounding context). Formatting the
  full document and extracting the range gives consistent
  results. The cost of formatting the full document is
  negligible for typical YAML files.
