**Repository:** root
**Status:** InProgress
**Created:** 2026-04-04

## Goal

Add diagnostic suppression comments so users can silence specific warnings
per-line or per-file. This gives escape hatches for false positives and
intentional style deviations without disabling validation globally. RedHat's
yaml-language-server has `# yaml-language-server-disable` — we need parity.

## Context

- No suppression mechanism exists currently — all diagnostics are always
  emitted
- The diagnostic pipeline runs in `server.rs::parse_and_publish()` (line 313)
  which collects diagnostics from parser, validators, and schema validation
- Modeline parsing already exists in `schema.rs` for `$schema=` and `$tags=`,
  scanning first 10 lines with prefix `# yaml-language-server:`
- All diagnostics have a string `code` field (e.g., `duplicateKey`, `flowMap`,
  `schemaRequired`) which can be used for targeted suppression
- Related: RedHat supports `# yaml-language-server: $schema=` modelines —
  we already support this format

### Suppression syntax (proposed)

Follow the established `# yaml-language-server:` prefix:
- **Per-line:** `# rlsp-yaml-disable-next-line [code1, code2]`
- **Per-file:** `# rlsp-yaml-disable-file [code1, code2]`
- **No code = suppress all:** `# rlsp-yaml-disable-next-line` suppresses
  all diagnostics on the next line

### Key files

- `rlsp-yaml/src/server.rs` — `parse_and_publish()` diagnostic pipeline
- `rlsp-yaml/src/validators.rs` — validator functions return `Vec<Diagnostic>`
- `rlsp-yaml/src/schema_validation.rs` — schema validation diagnostics
- `rlsp-yaml/src/schema.rs` — modeline parsing (pattern to follow)

## Steps

- [x] Implement suppression comment parser (63daa76)
- [x] Integrate with diagnostic pipeline (26fb7c7)
- [x] Add per-line suppression (26fb7c7)
- [x] Add per-file suppression (26fb7c7)
- [x] Add tests (26fb7c7)
- [x] Update configuration docs (470cc12)

## Tasks

### Task 1: Implement suppression comment parser (63daa76)

Add a function to scan YAML text for suppression comments and build a
suppression map. Follow the modeline scanning pattern in `schema.rs`.

- [x] Parse `# rlsp-yaml-disable-next-line [codes]` → suppress line N+1
- [x] Parse `# rlsp-yaml-disable-file [codes]` → suppress entire file
- [x] Handle comma-separated codes: `# rlsp-yaml-disable-next-line duplicateKey, flowMap`
- [x] Handle no codes (suppress all): `# rlsp-yaml-disable-next-line`
- [x] Return a `SuppressionMap` struct with per-line and per-file rules
- [x] Unit tests for parser

### Task 2: Integrate suppression with diagnostic pipeline (26fb7c7)

Filter diagnostics through the suppression map before publishing.

- [x] Call suppression parser in `parse_and_publish()`
- [x] Filter `Vec<Diagnostic>` — remove diagnostics matching suppression rules
- [x] Match by line number (per-line) and diagnostic code (per-code)
- [x] Per-file suppression filters all matching codes from entire file
- [x] Integration tests with LSP lifecycle

### Task 3: Update documentation (470cc12)

- [x] Add suppression syntax to `docs/configuration.md`
- [x] Add examples for common suppression patterns

## Decisions

- **Prefix `rlsp-yaml-disable`** not `yaml-language-server-disable` — we're
  a different tool with our own namespace. Users migrating from RedHat's LS
  would need to update comments, but this avoids ambiguity.
- **Per-line uses next-line semantics** — the comment suppresses the line
  BELOW it, not the line it's on. This is the standard pattern
  (eslint-disable-next-line, clippy allow).
- **Suppression applies post-collection** — all diagnostics are collected
  normally, then filtered. This keeps validator code clean (no suppression
  awareness needed in validators).
