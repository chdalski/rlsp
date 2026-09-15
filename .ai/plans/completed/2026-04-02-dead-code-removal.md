# Dead Code Removal

**Repository:** root
**Status:** Completed (2026-04-02)
**Created:** 2026-04-02

## Goal

Remove dead pub functions and types from `schema.rs` that
were committed as part of completed plans but never wired
into the server. An audit found four dead items and one
unused type. Removing them reduces maintenance burden and
prevents future agents from assuming they are live.

## Context

- Audit report: `.ai/reports/2026-04-02-dead-code-and-incomplete-delivery.md`
- All dead code is in `rlsp-yaml/src/schema.rs`
- `SchemaDraft` enum (line 108) — parsed from `$schema`
  but never read by any validation or server logic. The
  architecture chose to normalize cross-draft differences
  at parse time instead, making draft detection redundant.
- `fetch_schema` (line 475) — thin wrapper around
  `fetch_schema_raw` that discards the raw JSON Value.
  Superseded when the server switched to `fetch_schema_raw`
  in commit `3d10310`. Only called in tests.
- `check_vocabulary` (line 673) and `KNOWN_VOCABULARIES`
  (line 29) — vocabulary warning function that returns
  `Vec<String>`. Never called outside tests. If vocabulary
  warnings are needed later, they should produce LSP
  diagnostics, not plain strings. Also remove the
  `vocabulary` field from `JsonSchema` (line 177) and its
  parsing in `parse_extension_fields`.
- `parse_schema_with_remote` (line 659) — public entry
  point for remote `$ref` resolution. Never called from
  the server. However, `ParseContext` and its threading
  through all helpers is NOT dead — it is infrastructure
  that a future plan will wire in. Only remove the unused
  public wrapper function; keep `ParseContext` and the
  `Option<&mut ParseContext>` parameter threading.
- The `..Default::default()` pattern on `JsonSchema`
  construction in tests will need updating when the
  `draft` field is removed — the remaining fields still
  derive `Default` so this should be seamless.

## Steps

- [x] Remove `SchemaDraft`, `detect_draft`, `draft` field
- [x] Remove `fetch_schema`
- [x] Remove `check_vocabulary`, `KNOWN_VOCABULARIES`,
      `vocabulary` field and parsing
- [x] Remove `parse_schema_with_remote`
- [x] Remove all associated tests
- [x] Verify `cargo clippy --all-targets` and `cargo test`

## Tasks

### Task 1: Remove all dead code from schema.rs — `16a071a`

Single task because all removals are in one file, have no
dependencies between them, and the combined diff is small
enough to review atomically.

**File:** `rlsp-yaml/src/schema.rs`

**Removals:**

1. `SchemaDraft` enum (line 108) and its `Default` derive
2. `detect_draft` function (line 932)
3. `draft: SchemaDraft` field from `JsonSchema` (line 122)
4. The `schema.draft = ...` assignment in
   `parse_schema_with_root` (line 995)
5. 9 `detect_draft` / `SchemaDraft` tests (lines ~3433-3537)
6. `fetch_schema` function (line 475)
7. 2 `fetch_schema` tests (lines ~2211, ~2273) — migrate
   these to call `fetch_schema_raw` instead if they test
   meaningful behavior (SSRF blocking, error handling),
   or remove if they duplicate existing `fetch_schema_raw`
   tests
8. `KNOWN_VOCABULARIES` const (line 29)
9. `check_vocabulary` function (line 673)
10. `vocabulary: Option<HashMap<String, bool>>` field from
    `JsonSchema` (line 177)
11. `$vocabulary` parsing block in `parse_extension_fields`
    (the `schema.vocabulary = obj.get("$vocabulary")...`
    block)
12. 5 vocabulary tests (Vocab-1 through Vocab-5)
13. `parse_schema_with_remote` function (line 659)
14. 3 `parse_schema_with_remote` tests (lines ~3654, ~3817,
    ~3901) — check if these test `ParseContext` / remote
    ref behavior that is still needed. If so, refactor
    them to construct a `ParseContext` and call
    `parse_schema_with_root` directly. If they only test
    the wrapper, remove them.

**Verification:**
- [x] `cargo fmt`
- [x] `cargo clippy --all-targets` — zero warnings
- [x] `cargo test` — all tests pass
- [x] `cargo bench` — compiles (no need to run full suite)

## Decisions

- **Keep `ParseContext` and threading** — the remote `$ref`
  resolution infrastructure (`ParseContext`, `as_deref_mut`
  threading through ~15 helpers) is not dead. It is
  activated when a `Some(ctx)` is passed, which currently
  only happens in tests. A separate plan will wire this
  into the server. Removing and re-adding it would be
  wasteful churn.

- **Remove vocabulary entirely** — `check_vocabulary`
  returns `Vec<String>`, not LSP diagnostics. If vocabulary
  warnings are added later, they need a full redesign as
  proper diagnostics with severity, range, and code. The
  `vocabulary` field on `JsonSchema` is also only consumed
  by `check_vocabulary`, so it can go too.

- **Single task** — all removals are deletions in one file
  with no behavioral changes. Splitting into multiple
  tasks would add commit overhead for tightly coupled
  removals.
