**Repository:** root
**Status:** NotStarted
**Created:** 2026-03-16

## Goal

Add a duplicate key validator to the YAML LSP server that
emits error-level diagnostics when the same mapping key
appears more than once within the same mapping block. This
catches a common YAML authoring bug where the second
occurrence silently shadows the first.

## Context

- saphyr's `YamlOwned::Mapping` silently deduplicates keys
  (keeps last occurrence), so the parsed AST cannot be used
  for duplicate detection. The validator must work on raw
  text, similar to anchor/alias detection.
- The existing validator infrastructure in `validators.rs`
  follows a consistent pattern: public `validate_*` function
  returns `Vec<Diagnostic>`, text-based scanning with line
  iteration, diagnostics use `source: "rlsp-yaml"` and a
  string diagnostic code.
- `server.rs` wires validators in `parse_and_publish()` —
  the new validator call goes there.
- Design decisions from clarification:
  - **Severity:** Error (YAML spec forbids duplicate keys)
  - **Merge keys (`<<`):** Ignore — only flag textual
    duplicates within the same mapping block
  - **Configurability:** Always on (not configurable)

Key challenge: determining which keys belong to the same
mapping from raw text. Approach: track indentation levels.
Keys at the same indentation that aren't nested under a
different parent are siblings in the same mapping. Need to
handle:
- Top-level keys (indent 0)
- Nested mapping keys (same indent under same parent)
- Flow mappings (`{key1: v, key1: v}`) — duplicates within
  flow syntax
- Multi-document files (`---` boundaries reset scope)
- Keys after sequence items (`- key: val`)

## Steps

- [x] Clarify requirements with user
- [x] Review existing validator patterns and codebase
- [ ] Implement `validate_duplicate_keys` in validators.rs
- [ ] Wire into server.rs parse_and_publish
- [ ] Add tests

## Tasks

### Task 1: Implement duplicate key validator with tests and server wiring

Add `validate_duplicate_keys` function to `validators.rs`
following the existing validator pattern. Wire it into
`server.rs` `parse_and_publish()`. Include comprehensive
tests.

- [ ] Add `validate_duplicate_keys(text: &str) -> Vec<Diagnostic>` to validators.rs
- [ ] Handle block-style mappings via indentation tracking
- [ ] Handle flow-style mappings (`{key: v, key: v}`)
- [ ] Scope duplicate detection per mapping (not globally)
- [ ] Scope detection per document (`---` boundaries)
- [ ] Skip keys in comments and quoted strings
- [ ] Diagnostic code: `"duplicateKey"`, severity: ERROR
- [ ] Wire into `parse_and_publish` in server.rs
- [ ] Tests: no duplicates (clean file), simple duplicate at top level,
      nested mapping duplicates, multi-document scoping, flow mapping
      duplicates, keys after sequence items, quoted keys, edge cases

## Decisions

- **Text-based approach:** saphyr deduplicates keys in its
  AST, so we must scan raw text. This matches the pattern
  used by anchor/alias detection.
- **Indentation-based scoping:** Block mapping membership
  is determined by indentation level. Keys at the same
  indent under the same parent belong to the same mapping.
- **Single task:** This is a contained feature that fits in
  one vertical slice — the validator function, its tests,
  and the server wiring are tightly coupled and small
  enough for one commit.
