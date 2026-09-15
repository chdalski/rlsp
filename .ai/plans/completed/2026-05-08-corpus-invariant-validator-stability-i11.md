**Repository:** root
**Status:** Completed (2026-05-08)
**Created:** 2026-05-08

# Corpus invariant I11: validator stability under format-equivalent re-emit

## Goal

Add a corpus invariant that catches validators whose
diagnostic output depends on raw text layout rather than
the AST. For each corpus file and each of the seven
validators (six built-ins plus `validate_schema`), run
the validator twice — once on the original text and once
on the formatter's re-emission of the same text — and
assert that the two diagnostic multisets are identical
modulo range positions. Multiset comparison (rather than
set) is required so that duplicate-emission regressions
fail the invariant: if one variant emits the same
diagnostic twice and the other emits it once, that is a
failure even though the underlying set is identical. A
validator that consumes the AST correctly produces the
same diagnostics across an AST-preserving re-emit; a
validator that peeks at raw text or whitespace does not.
The invariant lands in
`rlsp-yaml/tests/corpus_invariants.rs` as I11 (next free
slot after I10).

## Context

### Existing harness

- `rlsp-yaml/tests/corpus_invariants.rs` registers 10
  invariants (I1 – I10) over the four files in
  `rlsp-yaml/tests/corpus/` (`docker-compose.yml`,
  `github-actions-matrix.yml`,
  `kubernetes-deployment.yaml`,
  `release-plz-workflow.yml`).
- I10 (`check_i10_formatter_round_trip`) already proves
  `format_yaml(text)` re-parses to an AST equivalent to
  parsing `text`. I11 layers on top: same re-emit, but
  compare diagnostic sets instead of AST shape.
- `collect_all_diagnostics` in the test file aggregates
  diagnostics from the six built-in validators
  (`validate_unused_anchors`, `validate_flow_style`,
  `validate_custom_tags`, `validate_key_ordering`,
  `validate_duplicate_keys`, `validate_yaml11_compat`).
  It does not call `validate_schema`.
- The skip-list is shrink-only and the Surprise Failure
  Protocol requires that any newly-failing
  `(file, invariant)` pair be reported to the lead, who
  files a follow-up plan; the developer then adds a
  skip-list entry citing that plan's file path. New
  entries without a referenced plan are forbidden. The
  human-readable mirror is
  `rlsp-yaml/tests/corpus/WORKLIST.md`.

### Validators in scope

I11 covers all seven validators users can have active
through normal LSP configuration:

- `validate_unused_anchors` (no settings)
- `validate_flow_style(&docs, &ValidationSettings::default())`
- `validate_custom_tags(&docs, &allowed_tags)` with empty
  allow-set (matches the harness pattern in I1)
- `validate_key_ordering`
- `validate_duplicate_keys(&docs, &ValidationSettings::default())`
- `validate_yaml11_compat`
- `validate_schema(&docs, &schema, format_validation = false, YamlVersion::V1_2)`
  — entry point in `rlsp-yaml/src/schema_validation.rs`.

`validate_schema` requires a `JsonSchema` argument. It is
not part of the existing `collect_all_diagnostics` because
that helper has no schema to pass; I11 introduces its own
collector helper that includes schema validation.

### Synthetic schema

A single handcrafted schema applied to every corpus file:

```json
{
  "type": "object",
  "additionalProperties": { "type": "string" }
}
```

Purpose: produce non-trivial diagnostic output on every
corpus file (each file has nested objects and non-string
values, so `validate_schema` emits multiple type-mismatch
diagnostics). This makes the I11 stability comparison
exercise real diagnostic content rather than `[]
== []`. The schema is constructed inline via
`rlsp_yaml::schema::parse_schema(&serde_json::json!(...))`,
following the pattern used by `configmap_schema()` in
`rlsp-yaml/tests/lsp_lifecycle.rs`.

### Stability assertion

A diagnostic's identity for I11 is the tuple
`(code, severity, message)`:

- `code` — `tower_lsp::lsp_types::Diagnostic.code`,
  formatted via `{:?}` on `Option<NumberOrString>` so both
  numeric and string codes compare reliably.
- `severity` — `Diagnostic.severity` (`Option<DiagnosticSeverity>`).
- `message` — `Diagnostic.message` as-is.
- `range` is **excluded** from the identity — ranges legitimately
  shift across re-emit because byte/column positions move.

The assertion compares diagnostic sets as **multisets**:
the same identity may appear N times pre-format and must
appear N times post-format. Multiset (rather than set)
catches duplicate-emission regressions where one variant
emits a diagnostic twice and the other emits it once. A
multiset is implemented as a sorted `Vec<(String, Option<DiagnosticSeverity>, String)>`
with `Vec::sort` then equality compare; sorting is
sufficient because the elements are total-orderable.

### Failure handling

The invariant lands with an empty skip-list. If the
existing corpus surfaces real bugs (a validator that
genuinely depends on whitespace), the developer follows
the Surprise Failure Protocol documented at the top of
`corpus_invariants.rs`:

1. Pause and report the `(file, I11)` pair plus failure
   detail to the lead via `SendMessage`.
2. Lead investigates and either (a) files a follow-up
   plan for the underlying validator bug and returns the
   plan path, or (b) directs that the failure is in
   scope of this plan.
3. Developer adds a skip-list entry citing the returned
   plan path, and updates `WORKLIST.md` accordingly.
4. Developer continues with the remaining work.

This protocol is unchanged from prior invariant plans
(e.g., I10). It exists so I11 can land immediately rather
than waiting for every potential bug to be diagnosed and
fixed first.

### References

- Existing pattern: I10 implementation in
  `rlsp-yaml/tests/corpus_invariants.rs:731-768`.
- Schema construction pattern:
  `configmap_schema()` in
  `rlsp-yaml/tests/lsp_lifecycle.rs:2562-2577`.
- `validate_schema` signature:
  `rlsp-yaml/src/schema_validation.rs:225-244`.
- Surprise Failure Protocol: module-level doc comment in
  `rlsp-yaml/tests/corpus_invariants.rs:1-18` and skip-list
  discipline in `rlsp-yaml/tests/corpus/WORKLIST.md:9-20`.

## Steps

- [x] Add `check_i11_validator_stability_under_reemit` to
  `rlsp-yaml/tests/corpus_invariants.rs`
- [x] Add a private helper that runs all seven validators
  (six built-ins + `validate_schema` with the synthetic
  schema) and returns a sorted multiset key vector
- [x] Add a private helper that builds the synthetic
  schema once
- [x] Register I11 in the `INVARIANTS` array
- [x] Add unit tests for the multiset-comparison helper
  (matching, mismatching, duplicate-count differences)
- [x] Run the harness; on any surprise failure, follow the
  Surprise Failure Protocol (report to lead, receive
  follow-up plan path, add skip-list entry plus
  `WORKLIST.md` row)
- [x] `cargo fmt`, `cargo clippy --all-targets`,
  `cargo test -p rlsp-yaml --test corpus_invariants` all
  pass with zero warnings

## Tasks

### Task 1: Add I11 corpus invariant for validator stability under format-equivalent re-emit

**Commit:** `1fabc706a251aac01832e3248408fc609af387db`

Implement the new corpus invariant that runs all seven
validators on a corpus file's original text and on
`format_yaml(text)`, then asserts the two diagnostic
multisets are identical modulo range. This is a single
committable slice — invariant code, helper functions,
unit tests, and registration land together.

- [x] Add a private function in
  `rlsp-yaml/tests/corpus_invariants.rs` that builds the
  synthetic schema by calling
  `rlsp_yaml::schema::parse_schema(&serde_json::json!({
    "type": "object",
    "additionalProperties": { "type": "string" }
  }))` and unwrapping with `.expect(...)` at the test
  call site. Match the in-test schema construction style
  from `configmap_schema()` in `lsp_lifecycle.rs`.
- [x] Add a private function `i11_collect_diagnostics(docs, schema)`
  that runs all six built-in validators (mirroring the
  call shapes in `collect_all_diagnostics`) plus
  `validate_schema(docs, schema, false, YamlVersion::V1_2)`
  and returns the concatenated `Vec<Diagnostic>`. Do not
  modify the existing `collect_all_diagnostics` — other
  invariants depend on its current behavior.
- [x] Add a private function `diagnostic_identity_multiset(diags)`
  that returns a sorted
  `Vec<(String, Option<DiagnosticSeverity>, String)>`
  where each tuple is `(format!("{:?}", d.code), d.severity, d.message.clone())`.
- [x] Add `check_i11_validator_stability_under_reemit(_path, text) -> Result<(), String>`:
  parse `text`; if the document set is empty, return
  `Ok(())` (matches I10's empty-input behavior and avoids
  asserting on degenerate parses). Otherwise build the
  schema, collect the pre-format multiset, run
  `format_yaml(text, &YamlFormatOptions::default())`,
  parse the formatted text (return `Err("formatter output
  failed to parse")` if it parses to zero documents,
  matching I10's error message form), collect the
  post-format multiset, and compare. On mismatch return
  an error string identifying the first differing
  diagnostic identity and whether it is missing,
  duplicated, or new.
- [x] Append a new `Invariant { id: "I11", description:
  "Validator stability under format-equivalent re-emit:
  diagnostic identities (code, severity, message) match
  pre- and post-format on AST-equivalent input", check:
  check_i11_validator_stability_under_reemit }` entry to
  the `INVARIANTS` array.
- [x] Add unit tests in the existing `mod tests` block
  for `diagnostic_identity_multiset`:
  - identical inputs produce equal multisets
  - reordering the input vector does not change the
    multiset
  - differing message text produces different multisets
  - same identity appearing twice in one input but once
    in the other produces different multisets
  - empty input produces an empty multiset
- [x] Add a unit test that asserts the I11 entry exists
  in `INVARIANTS` with id `"I11"`. This guards against
  accidental removal during future refactors.
- [x] Run `cargo test -p rlsp-yaml --test corpus_invariants`.
  If a `(file, I11)` pair fails on the existing four
  corpus files, follow the Surprise Failure Protocol:
  pause, report the failure detail to the lead, wait for
  a follow-up plan path, then add a `SKIP_LIST` entry
  `(file_name, "I11", plan_path_string)` and append a
  matching row to `rlsp-yaml/tests/corpus/WORKLIST.md`.
- [x] After all corpus files either pass I11 or are
  skip-listed with referenced plans, the test command
  must pass: `cargo test -p rlsp-yaml --test corpus_invariants`
  exits 0 with the harness reporting `11 invariants × 4
  files = 44 checks`.
- [x] `cargo fmt` produces no diff
- [x] `cargo clippy --all-targets -p rlsp-yaml` reports
  zero warnings

## Decisions

- **Re-emit via `format_yaml` (not a custom whitespace-only pass).**
  I10 already proves `format_yaml` preserves AST
  structure across the corpus. Validators consume the
  AST, so a validator whose output changes across
  `format_yaml` is operating on raw text — exactly the
  bug class I11 targets. The formatter perturbs strictly
  more than whitespace (quote style, flow/block), which
  catches a strictly-larger bug class than a pure
  whitespace re-emitter and avoids new test-only
  re-emitter code. The invariant is named "validator
  stability under format-equivalent re-emit" to reflect
  this; the original follow-up entry's "whitespace-only"
  phrasing was a stylistic framing rather than a hard
  requirement.

- **All seven validators in scope, including
  `validate_schema`.** The bug class — "validator output
  depends on text layout" — is testable on every
  validator that reads YAML structure. Schema validation
  reads through the same AST, so excluding it would
  leave a meaningful coverage gap.

- **Single synthetic schema for all four corpus files.**
  The schema's purpose is to drive `validate_schema` into
  a non-trivial diagnostic output, not to validate that
  the corpus files conform to any particular real-world
  schema. A small handcrafted schema requiring
  string-typed `additionalProperties` produces type-
  mismatch diagnostics on every corpus file (each has
  nested objects and non-string values). Vendoring real
  Schema Store assets (github-workflow, kubernetes,
  compose-spec) would balloon fixture footprint and
  invite drift maintenance for no gain to this
  invariant.

- **Diagnostic identity is `(code, severity, message)`,
  compared as a multiset.** Range is excluded because
  positions legitimately shift across formatting.
  Multiset (rather than set) catches duplicate-emission
  regressions. Severity is included because a validator
  switching a diagnostic's severity (Warning → Error or
  vice versa) on a layout change is a bug worth
  catching. False-positive risk on `message` is low —
  validator messages are constructed from AST data
  (anchor names, key strings, scalar values), all of
  which I10 proves stable across `format_yaml`.

- **Surprise Failure Protocol for existing-corpus
  failures.** I11 lands with an empty skip-list. If a
  failure surfaces during initial test runs, the
  developer reports it to the lead, the lead files a
  follow-up plan for the underlying validator bug, and
  the developer adds a referenced skip-list entry. This
  matches the discipline established for prior
  invariants and is enforced by the harness's existing
  shrink-only constraint.

- **Helper functions live in `corpus_invariants.rs`, not
  `tests/common/mod.rs`.** The existing invariant file
  defines its own helpers locally (it does not import
  `mod common;`) and following the same convention keeps
  this plan's scope minimal. Cross-file extraction can be
  a separate consolidation plan if invariants in other
  test files end up needing the same machinery.
