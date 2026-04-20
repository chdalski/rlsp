**Repository:** root
**Status:** Completed (2026-04-20)
**Created:** 2026-04-20

# Speed up parser_boundary_audit test

## Goal

The `parser_boundary_audit` integration test in
`rlsp-yaml/tests/parser_boundary_audit.rs` takes ~47 seconds to run — by
itself it accounts for ~92% of `cargo test` wall-clock time (the other 46
tests in its binary finish in under 20 ms, and every other test binary
in the workspace combined finishes in under 4 s). The slowness comes
entirely from compiling the same three regex patterns tens of thousands
of times inside helper functions called per line of scanned source. Lift
those regexes into `LazyLock` statics so each pattern compiles once per
test run. The test is a permanent enforcement mechanism (shrink-only per
its own docstring) that rides along with every future `cargo test`
invocation, so this cost otherwise compounds indefinitely.

## Context

### The hot path

Three regex constructors sit inside helper functions that are on the
per-line hot path of the audit:

| Location | Pattern | Calls per run |
|---|---|---|
| `parser_boundary_audit.rs:425` inside `is_candidate_fn_line` | `^(?:pub\s+)?fn\s+\w` | one per source line |
| `parser_boundary_audit.rs:459` inside `has_text_str_param` | `^\s*&(?:'[a-z_]+\s+)?(?:mut\s+)?self\s*,\s*` | one per candidate `fn` line |
| `parser_boundary_audit.rs:463` inside `has_text_str_param` | `^\s*(?:text\|line\|lines\|content\|source\|input)\s*:\s*(?:&str\b\|&\[&str\])` | one per candidate `fn` line |

Measured against the current `rlsp-yaml/src/` tree:

- 31 `.rs` files, ~35,434 total lines → ~35,434 calls to
  `is_candidate_fn_line`
- ~1,638 `fn` declaration lines → ~1,638 calls to `has_text_str_param`
- Total regex compilations per test run: **~38,710**
- Observed wall time (debug build, single test,
  `--exact parser_boundary_audit`): **47.79 s**

### Why this is worth fixing even as the "One parser, one AST" retrofits land

The audit is a permanent enforcement mechanism. Per its own docstring
(lines 24–32 of the test file), the allow-list is **shrink-only** —
retrofits remove entries, but the audit keeps running to prevent new
violations. The slowness is proportional to source-tree size, not to
allow-list length or violation count, so retrofitting individual
`rlsp-yaml/src/` features does not reduce the cost. This fix is
orthogonal to every retrofit plan listed in
`.ai/memory/project_followup_plans.md`.

### Existing project convention

`rlsp-yaml/src/decorators/document_links.rs:14-17` already uses this
pattern at module scope:

```rust
static URL_REGEX: std::sync::LazyLock<Regex> = std::sync::LazyLock::new(|| {
    Regex::new(r#"..."#)
        .unwrap_or_else(|_| unreachable!("static regex is valid"))
});
```

Match this style for consistency. Place each static **inside** the helper
function that uses it (per the `regex` crate's own documented pattern
and the YAGNI principle — no cross-function visibility needed).

### References

- `https://docs.rs/regex/latest/regex/#avoid-re-compiling-regexes-especially-in-a-loop`
  — the canonical anti-pattern guidance and recommended `LazyLock` form.
  Upstream example:

  ```rust
  use std::sync::LazyLock;
  use regex::Regex;

  fn some_helper_function(haystack: &str) -> bool {
      static RE: LazyLock<Regex> = LazyLock::new(|| Regex::new(r"...").unwrap());
      RE.is_match(haystack)
  }
  ```
- `https://doc.rust-lang.org/std/sync/struct.LazyLock.html` — stable
  since Rust 1.80; current toolchain is 1.95.
- `rlsp-yaml/src/decorators/document_links.rs` — existing project
  convention for static `LazyLock<Regex>`.

## Steps

- [x] Re-measure baseline timing to confirm the ~47 s figure on a
  freshly-built test binary
- [x] Lift the three regexes into function-local `LazyLock` statics in
  `parser_boundary_audit.rs`
- [x] Run `cargo test -p rlsp-yaml --test parser_boundary_audit` and
  confirm all 47 tests still pass
- [x] Measure post-fix timing and confirm the acceptance threshold is met
- [x] Run `cargo fmt` and `cargo clippy --all-targets` — zero warnings
  required per workspace lints
- [x] Commit the change

## Tasks

### Task 1: Cache the three regexes with `LazyLock`

**Completed** — commit `7b8383fa9781455ef3d6b5a12c0aff702d047442`. Measurements: `parser_boundary_audit` alone dropped from 47.79 s to 0.03 s; full `cargo test` dropped to 2.67 s.

Replace the three `Regex::new(...).unwrap()` calls inside
`is_candidate_fn_line` and `has_text_str_param` with function-local
`static RE_*: LazyLock<Regex>` bindings so each pattern compiles once
per test-binary run instead of once per call.

- [x] Cache the candidate-fn regex (currently `parser_boundary_audit.rs:425`):

  ```rust
  fn is_candidate_fn_line(line: &str) -> bool {
      static CANDIDATE_FN_REGEX: std::sync::LazyLock<Regex> =
          std::sync::LazyLock::new(|| {
              Regex::new(r"^(?:pub\s+)?fn\s+\w")
                  .unwrap_or_else(|_| unreachable!("static regex is valid"))
          });
      CANDIDATE_FN_REGEX.is_match(line.trim_start())
  }
  ```

- [x] Cache the self-receiver regex (currently `parser_boundary_audit.rs:459`):

  ```rust
  static SELF_RECEIVER_REGEX: std::sync::LazyLock<Regex> = ...
      Regex::new(r"^\s*&(?:'[a-z_]+\s+)?(?:mut\s+)?self\s*,\s*")
  ```

- [x] Cache the text-param regex (currently `parser_boundary_audit.rs:463`):

  ```rust
  static TEXT_PARAM_REGEX: std::sync::LazyLock<Regex> = ...
      Regex::new(r"^\s*(?:text|line|lines|content|source|input)\s*:\s*(?:&str\b|&\[&str\])")
  ```

- [x] `has_text_str_param` uses both `SELF_RECEIVER_REGEX` and
  `TEXT_PARAM_REGEX` — both statics live inside that function.
- [x] Do not introduce new dependencies (`once_cell`, `lazy_static`,
  `regex_static`, `lazy-regex`). `std::sync::LazyLock` is stable on
  the workspace toolchain.
- [x] Do not change any regex pattern text, function signatures, or
  call-site wording — behavior must be identical.

**Acceptance criteria (all must hold):**

- [x] All 47 tests in
  `target/debug/deps/parser_boundary_audit-*` pass (46 `detection_tests::*`
  + `parser_boundary_audit`)
- [x] The `parser_boundary_audit` test alone
  (`cargo test -p rlsp-yaml --test parser_boundary_audit -- --exact parser_boundary_audit`)
  finishes in **under 500 ms** wall time on the same machine and build
  profile that currently measures ~47 s (a ≥94× speedup; the conservative
  500 ms threshold leaves margin for I/O variance and does not lock in
  a specific multiple) — **measured: 0.03 s**
- [x] Full `cargo test` workspace wall time drops from the pre-change
  baseline (captured in Step 1) to **under 10 s** on the same machine —
  **measured: 2.67 s**
- [x] No new dependencies in any `Cargo.toml`
- [x] `cargo fmt` produces no diff
- [x] `cargo clippy --all-targets` reports zero warnings
- [x] `git diff --stat rlsp-yaml/tests/parser_boundary_audit.rs` shows a
  small, localized change (the three affected functions only; no
  unrelated edits) — **+16/-11 lines, one file**

## Decisions

- **`std::sync::LazyLock` over `once_cell::sync::Lazy`** — stable in
  std since Rust 1.80; current workspace toolchain is 1.95; adding
  `once_cell` would be an unnecessary dependency. This matches the
  `regex` crate's own documented recommendation and the existing
  `URL_REGEX` static in `rlsp-yaml/src/decorators/document_links.rs`.
- **Function-local `static` bindings, not module-level** — per the
  `regex` crate's documented pattern. Each regex is used in exactly
  one function; hoisting them to module scope adds visibility the
  code doesn't need and separates the pattern from its use site.
- **`.unwrap_or_else(|_| unreachable!("static regex is valid"))`
  over bare `.unwrap()`** — matches the existing project convention in
  `document_links.rs` and produces a slightly more actionable panic
  message if a pattern is ever mis-edited. The file-level
  `#![expect(clippy::unwrap_used, reason = ...)]` already permits
  `.unwrap()`, so either form would compile; consistency with
  existing code wins.
- **No broader cleanup in scope** — the audit also has an
  `ALLOW_LIST.iter().find(...)` linear scan and walks the source tree
  with recursive `fs::read_dir`. Both are microseconds to milliseconds
  compared to the 47 s regex cost; fixing them now would broaden the
  task and violate YAGNI. If measurements after the fix show the test
  is still slow, a follow-up plan can address those.
- **No new tests** — the 46 `detection_tests::*` functions already
  exercise every detection helper. This change moves regex
  construction but does not change detection logic, so existing
  coverage is sufficient.

## Non-Goals

- **Fixing any alleged `analysis/semantic_tokens.rs::parse_docs`
  violation.** The audit currently passes (re-measured during
  clarification); there is no open violation. Any future violation
  that appears is a separate decision (retrofit vs. allow-list entry)
  owned by the relevant retrofit plan in
  `.ai/memory/project_followup_plans.md`.
- **Refactoring `collect_violations`, `scan_file`, or the
  `ALLOW_LIST` lookup.** Out of scope per the Decisions above.
- **Adding benchmarks (Criterion or otherwise) for this test.** The
  acceptance criteria specify concrete wall-time thresholds; a one-off
  before/after measurement is sufficient.
- **Touching any other file.** The entire change is confined to
  `rlsp-yaml/tests/parser_boundary_audit.rs`.
