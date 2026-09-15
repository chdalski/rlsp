**Repository:** root
**Status:** Completed (2026-05-07)
**Created:** 2026-05-07

# LSP invalidCharacter Diagnostic Code

## Goal

Surface non-printable character parse errors with a
distinct LSP diagnostic code `invalidCharacter` so editors
can filter, theme, or quick-fix them independently from
generic `yamlSyntax` grammar errors. The parser hygiene
plan (commit 4763e70) already exposes
`ErrorKind::InvalidCharacter` on `LoadError::Parse` — this
plan wires it to the LSP diagnostic code and documents it.

## Context

- The parser hygiene plan (`.ai/plans/2026-05-07-parser-error-api-hygiene.md`,
  completed 2026-05-07) added `ErrorKind` to `Error` and
  `LoadError::Parse`. `ErrorKind::InvalidCharacter` covers
  c-printable, nb-json, and escape-produces-non-printable
  violations. `ErrorKind::Syntax` covers everything else.
- `rlsp-yaml/src/parser.rs:35-69` currently matches
  `LoadError::Parse { pos, message, .. }` and hardcodes
  `code: "yamlSyntax"` for all parse errors. The `..`
  rest pattern (from Task 1, commit 90e5b7c) already
  absorbs the `kind` field — the code just needs to read
  it.
- `rlsp-yaml/docs/configuration.md:471-496` documents all
  available diagnostic codes in a table. `yamlSyntax` is
  listed at line 482.
- The suppression system (`rlsp-yaml/src/validation/suppression.rs`)
  accepts any string code — `invalidCharacter` will be
  automatically suppressible with
  `# rlsp-yaml-disable invalidCharacter`.
- The diagnostic-driven code-action dispatch in
  `rlsp-yaml/src/editing/code_actions.rs:56-76` has a
  `_ => vec![]` fallback for unrecognized codes — no
  change needed there.
- The follow-up item is tracked in
  `.ai/memory/project_followup_plans.md` under
  `## Open: rlsp-yaml` as "Non-printable unicode character
  diagnostic (LSP layer only)."

### Readers of the changed code path

- `server.rs:469` calls `parser::parse_yaml(text)` and
  extends the diagnostic vector with its result. The
  diagnostic `code` field flows unchanged to
  `publish_diagnostics()`.
- `validation/suppression.rs` reads `diag.code` as a
  string for suppression matching — works with any code.
- `editing/code_actions.rs:56-76` dispatches on
  `diagnostic_code(diag)` — has a `_ => vec![]` catch-all.
- Existing `parser.rs` tests (lines 81-421) test
  `parse_yaml()` output for diagnostic fields.

The suppression filter in `server.rs:534` also reads
`diag.code` as a string for all diagnostics (including
parse-error diagnostics) and passes it to `is_suppressed()`.
Since suppression accepts any string code,
`invalidCharacter` is automatically suppressible — no
suppression code changes needed. No other code reads or
dispatches on the `code` field of parse-error diagnostics
beyond the four sites listed above and the suppression
filter.

## Steps

- [x] Map `ErrorKind::InvalidCharacter` to diagnostic code
  `"invalidCharacter"` in `rlsp-yaml/src/parser.rs`; keep
  all other parse errors as `"yamlSyntax"`
- [x] Add unit tests verifying the code mapping
- [x] Add `invalidCharacter` to the diagnostic codes table
  in `rlsp-yaml/docs/configuration.md`; update the
  `yamlSyntax` description
- [x] Add a `feature-log.md` entry for the new diagnostic
  code
- [x] Remove the follow-up item from
  `.ai/memory/project_followup_plans.md`

## Tasks

### Task 1: Wire invalidCharacter diagnostic code

Map `ErrorKind::InvalidCharacter` from `LoadError::Parse`
to diagnostic code `"invalidCharacter"` in the LSP layer.
Add unit tests. Update documentation. Remove the follow-up
tracking item.

- [x] In `rlsp-yaml/src/parser.rs:36`, change the `Parse`
  match arm to extract `kind` and compute the diagnostic
  code: `ErrorKind::InvalidCharacter` → `"invalidCharacter"`,
  `ErrorKind::Syntax` and `_ =>` → `"yamlSyntax"`
- [x] Use the computed code at line 69 instead of the
  hardcoded `"yamlSyntax"` string
- [x] Add a unit test: `parse_yaml("# comment \x80\n")`
  produces a diagnostic with
  `code: Some(NumberOrString::String("invalidCharacter"))`
- [x] Add a unit test: `parse_yaml("key: [bad\n")` still
  produces `code: Some(NumberOrString::String("yamlSyntax"))`
- [x] Add `invalidCharacter` row to the diagnostic codes
  table in `rlsp-yaml/docs/configuration.md` (after the
  `yamlSyntax` row): code `invalidCharacter`, description
  "Non-printable character not allowed by YAML 1.2
  character-set rules (c-printable, nb-json)"
- [x] Update the `yamlSyntax` row description from
  "YAML parse error" to "YAML grammar or structure error"
  (the generic category after `invalidCharacter` was
  split out)
- [x] Add a `feature-log.md` entry for the
  `invalidCharacter` diagnostic code in
  `rlsp-yaml/docs/feature-log.md` (newest-first ordering),
  following the Description / Complexity / Comment / Tier
  format — user-visible: editors can now distinguish
  character-set violations from grammar errors by code
- [x] Remove the "Non-printable unicode character
  diagnostic (LSP layer only)" item from
  `.ai/memory/project_followup_plans.md`
- [x] `cargo fmt`, `cargo clippy --all-targets --workspace`,
  `cargo build --workspace`, `cargo test --workspace`
  all pass with zero warnings

Acceptance: YAML input containing a non-printable character
(e.g., `# comment \x80\n`) produces a diagnostic with code
`"invalidCharacter"`; YAML input with a grammar error
produces `"yamlSyntax"`; the diagnostic codes table in
`configuration.md` lists both codes; `feature-log.md` has
a new entry for the `invalidCharacter` diagnostic; the
follow-up item is removed from `project_followup_plans.md`.

**Commit:** ebec83f

## Decisions

- **Single task** — the change is ~10 lines of production
  code, 2 unit tests, and 2 doc updates. Splitting into
  multiple tasks would add coordination overhead
  disproportionate to the work.
- **No configurable severity** — `invalidCharacter` is a
  YAML spec violation (like `yamlSyntax`). Always emitted
  at ERROR severity. Configurable severity is reserved for
  style diagnostics (flowStyle, duplicateKey).
- **No code-action for invalidCharacter** — there is no
  automated fix for a non-printable character in a YAML
  file. The diagnostic surfaces the issue; the user
  removes the character manually.
- **Unit tests on `parse_yaml()`, not LSP lifecycle** —
  `parse_yaml()` is the diagnostic-producing function;
  the server flow (`parse_and_publish`) forwards the
  diagnostic vector unchanged to `publish_diagnostics`.
  The wiring is already covered by existing lifecycle
  tests. Testing at the `parse_yaml()` level is sufficient
  and faster.

## Non-Goals

- Adding code actions for `invalidCharacter` (no
  meaningful automated fix).
- Configurable severity for `invalidCharacter` (spec
  violation, always ERROR).
- Adding more diagnostic codes for other `LoadError`
  variants (`nestingTooDeep`, `circularAlias`, etc.) —
  those are separate follow-ups if wanted.
- Changing the parser's `ErrorKind` enum or adding new
  variants.
