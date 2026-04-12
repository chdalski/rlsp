---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

## Open — Feature Work

1. **YAML version selection** — `yaml.yamlVersion` for 1.1 vs 1.2 boolean interpretation (`on`/`off`/`yes`/`no`)
2. **Flow style enforcement levels** — RedHat can forbid flow style (ERROR), we only warn. Add a severity setting on existing flowMap/flowSeq diagnostics.
3. **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.

## Open — Test Refactoring

4. **Expand rstest usage in rlsp-yaml-parser** — rstest is currently only used in `tests/conformance.rs` via `#[files(...)]`. The crate has ~648 `#[test]` functions across ~18,847 LOC, and large swaths are near-identical calls differing only by input/expected. Prime candidates (299 tests total, highest ROI): `src/lexer/plain.rs` (92), `src/lexer/quoted.rs` (92), `src/lexer/block.rs` (61), `src/lexer/lines.rs` (54). Larger but more heterogeneous: `tests/smoke.rs` (513). Moderate candidates: `tests/unicode_positions.rs` (21), `tests/encoding.rs` (31), `tests/loader.rs` (55). Estimated consolidation: ~812 test functions → ~80–150 parameterized tests.

   **Conversion rule — split over helpers.** When a group has mixed assertion shapes (`assert_eq!`, `matches!`, span-tuple comparisons), split it into multiple `#[rstest]` functions named after their assertion shape (e.g. `plain_scalar_cases_eq`, `plain_scalar_cases_matches`, `plain_scalar_cases_span`). Do **not** create comparable-type helpers that normalize diverse outputs into one unified return type — keep assertion shape obvious at the test site.

   **Leave alone:** `tests/robustness.rs`, `tests/error_reporting.rs`, `tests/loader_spans.rs` — test cases are genuinely heterogeneous, no structural repetition to consolidate.

   **Suggested staging:** start with the four lexer submodules as a first plan (cleanest patterns, uniform assertion shape, private-API tests already colocated), measure LOC reduction and CI-time impact, then decide whether to tackle smoke.rs as a follow-up.

## Open — Cleanup queue (accumulating from 2026-04-11 code-improvements plan)

Not committed yet — the user is deliberately accumulating cleanup findings during plan execution and will bundle them into a follow-up task or plan when the list feels complete. Do not create one-off tasks for individual items.

**Principle being applied:** Do not write `file.rs:NNN` references in comments. Line numbers decay as code moves; function names get renamed; files get split. A comment that says "loader.rs:399 — the anchor registration site" becomes misleading the moment anything shifts, and there's no build-time check to catch the drift. Reference by symbol name (function, type, method) and/or behavioral description instead — grep survives refactors that line numbers don't. Applies to source/test/doc comments and plan/design files; does NOT apply to commit messages (snapshots in time) or external docs pinned to a commit SHA.

**Cleanup item C1 — Stale `file.rs:NNN` references in comments (found 2026-04-11):** Three occurrences across the parser crate, all verified stale after the code-improvements plan's Task 7/8/9 refactorings shifted line numbers:

- `rlsp-yaml-parser/tests/robustness.rs:70` — comment says `loader.rs:399` ("loader registers mapping/sequence anchors AFTER parsing their content"). Current `loader.rs:399` is a stream-helper call, unrelated. Rewrite without the line number — describe the behavior by function/context (anchors are registered after `parse_node` finishes expanding their content, not at the `&anchor` token).
- `rlsp-yaml-parser/tests/robustness.rs:104` — comment says `loader.rs:528` ("CircularAlias code path"). Current `loader.rs:528` is a comment-event dispatch arm, unrelated. Rewrite by naming the function that contains the CircularAlias defense (check current location — likely `expand_node` or similar) and what it guards against.
- `rlsp-yaml-parser/tests/smoke.rs:8445` — comment says `consume_line` at `lib.rs:1124` ("unrecognised content path"). Current `lib.rs:1124` is anchor-skip logic in `inline_contains_mapping_key`, unrelated. Rewrite to describe the "unrecognised content" path by its current function name — the referenced function may have been renamed or moved.

**Verification step when fixing C1:** grep for `\b\w+\.rs:\d+` across the parser crate to confirm no new hits have been added since this memory was written. If additional hits appear, rewrite them the same way.

**Cleanup item C2 — if/else-if chains that should be `match` (found 2026-04-11):** Scan of the Rust crates turned up several readability issues where `if` chains comparing the same scalar against literals would read more cleanly as `match`. All findings are behaviourally sound today; the win is purely readability. Locations use function/symbol names, not line numbers (per the principle above).

**Verified findings (read in full during investigation):**

- **`rlsp-yaml-parser/src/lexer/block.rs` — `parse_block_header`** — the `Some(ch) =>` arm in the indicator loop uses `if ch == '+' { ... } else if ch == '-' { ... } else if ch.is_ascii_digit() { if ch == '0' { ... } }`. Refactor: flatten into the enclosing `match remaining.chars().next()` so every indicator class is its own pattern arm (`Some('+')`, `Some('-')`, `Some('0')`, `Some(ch @ '1'..='9')`, `Some(ch)`). Removes the nested `ch == '0'` check.

- **`rlsp-yaml/src/schema_validation.rs` — `yaml_type_name`** and **`rlsp-yaml/src/symbols.rs` — `node_symbol_kind`** — both contain the same is_null/is_bool/is_integer/is_float/else chain on plain scalar values, differing in three ways: (a) `schema_validation.rs` early-returns `"string"` for non-Plain `ScalarStyle`, `symbols.rs` has no style check; (b) `symbols.rs` collapses Integer+Float → `SymbolKind::NUMBER`, `schema_validation.rs` keeps `"integer"` and `"number"` distinct; (c) `Node::Alias` maps to `"unknown"` in schema_validation and `SymbolKind::STRING` in symbols. **Recommended shared refactor:** add `PlainScalarKind { Null, Bool, Integer, Float, String }` and `classify_plain(value: &str) -> PlainScalarKind` in `scalar_helpers`. The helper still calls the four predicates in order (that chain is inherent — each `is_X` is meaningful domain logic and cannot be pattern-matched away), but both call sites then collapse to a `match` on the enum and the classification order lives in one place. Do **not** match on a tuple of booleans — that was one Explore suggestion and it would be strictly worse than the current chain.

**Agent-reported, verify against the source before planning:**

- `rlsp-yaml/src/color.rs` — 4-way `starts_with` chain for `rgba(`/`rgb(`/`hsla(`/`hsl(` prefixes.
- `rlsp-yaml-parser/src/event_iter/line_mapping.rs` — UTF-8 byte-length classification via `& 0xE0 == 0xC0` / `& 0xF0 == 0xE0`; potentially `match ch.leading_ones()` or a guarded match.
- `rlsp-yaml/src/scalar_helpers.rs` — integer prefix parsing (`0o`, `0x`) as an `else if let Some(...) = strip_prefix(...)` chain.
- `rlsp-yaml/src/server.rs` — 3-way `else if let` schema-detection fallback chain. Lowest priority — borderline clear as-is.

**Out of scope for this cleanup:** behavioural divergences spotted during investigation (notably the `Node::Alias` mapping difference between `schema_validation.rs` and `symbols.rs`). The refactor must preserve current behaviour; whether alias-type semantics should be unified is a separate decision for the user.

**Cleanup item C3 — schema_validation.rs / symbols.rs scalar classification (found 2026-04-11):** Key investigation result on the two files you asked about: both have the same is_null/is_bool/is_integer/is_float chain but differ on (a) plain-style early-return, (b) integer/float collapse, and (c) Node::Alias mapping ("unknown" vs STRING). Recommended refactor is a shared PlainScalarKind enum + classify_plain helper in scalar_helpers — each call site becomes a match on the enum with its own collapse rules, and the predicate chain lives in one place. Flagged the alias divergence as out-of-scope for the refactor since it's a behavioural question.

**Cleanup item C4 — Iterator-style replacements in rlsp-yaml-parser (found 2026-04-12):**

- **C4a — `repeat_n` for newline pushes (4 sites):** Replace `for _ in 0..n { out.push('\n') }` with `out.extend(std::iter::repeat_n('\n', n))`. Locations: `lexer/quoted.rs` (`pending_blanks` push in fold-separator handling), `lexer/block.rs` (two `trailing_newlines` flushes — one in literal block content, one in folded block content), `lexer/plain.rs` (`pending_blanks` push in continuation handling).

- **C4b — `str::replace` chain for `normalize_line_breaks` (1 site):** Replace the manual `Peekable<Chars>` loop in `encoding.rs` `normalize_line_breaks` with `s.replace("\r\n", "\n").replace('\r', "\n")`. The fast-path (`!s.contains('\r')`) already short-circuits the common case; the two-pass replace is clearer than the manual peek-consume loop for the rare CR path.

- **C4c — `Iterator` impl for `LineBuffer` (1 impl, enables simplifications):** `LineBuffer` already has `consume_next() -> Option<Line>` matching the `Iterator::next` signature. Add `impl Iterator for LineBuffer` as a trivial delegation. Immediate simplification: `lexer.rs` `drain_to_end` becomes a `.map().last()` one-liner instead of a while-let loop.

- **C4d — `matches!` for `is_duplicate` in `event_iter/step.rs`:** Replace the if/else-if chain that builds the `is_duplicate` boolean with a single `matches!` expression over `(&self.pending_anchor, has_standalone_tag, had_inline)`.
