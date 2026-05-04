**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-04

## Goal

Improve error-position precision across 6 error classes identified by the Phase 2 error-and-limits audit (L12-L17), closing the last Phase 2 Lenient entries. Currently, several error classes report start-of-construct positions (`%` for directive errors, EOF for unterminated scalars, `&` for anchor overflow) rather than the offending byte, and 5 `LoadError` variants carry no position at all. After this fix, parser-side errors point to the precise offending byte, and loader errors include the position of the triggering node.

## Context

- **Phase 2 audit finding:** `.ai/audit/2026-04-30-phase2-prose/reconciliation-error-and-limits.md` Defect 1 (items 9-14) — 6 error classes with imprecise or missing positions.
- **The 6 error classes:**

  | # | Error class | Current pos | Target pos | Location |
  |---|-------------|-------------|------------|----------|
  | 1 | `%YAML` major-0 rejection | `%` (dir_pos) | Major digit | `directives.rs:191-195` |
  | 2 | `%YAML` u8 digit overflow | `%` (dir_pos) | Overflowing digit | `directives.rs:181-188` |
  | 3 | Unterminated single-quoted scalar | EOF (current_pos) | Opening `'` | `lexer/quoted.rs:116-119` |
  | 4 | Resolved-tag overflow | Tag indicator pos | The `!handle!` token | `directive_scope.rs:140-146` etc. |
  | 5 | `MAX_ANCHOR_NAME_BYTES` overflow | `&` (indicator_pos) | First byte beyond limit | `properties.rs:38-42` |
  | 6 | 5 `LoadError` variants | No `pos` field | Triggering node | `loader.rs:77-107` |

- **Existing precise-position model:** The implicit-key 1024-byte limit at `event_iter/flow.rs:1136-1161` demonstrates a feasible design — it captures the precise byte offset at the point where the limit is exceeded.
- **`LoadError` is a public enum.** Adding `pos` fields to 5 variants is a public API change. All consumers that pattern-match on the affected variants must be updated:
  - `rlsp-yaml-parser/src/loader.rs` — construction sites
  - `rlsp-yaml/src/parser.rs` — LSP diagnostic consumer
  - `rlsp-yaml-parser/tests/loader.rs` — loader unit tests
  - `rlsp-yaml-parser/tests/conformance/loader.rs` — conformance loader tests
  - `rlsp-yaml-parser/tests/robustness.rs` — robustness/DoS tests
- **Performance:** All fixes are on error paths only — byte-offset arithmetic that runs zero times on valid input. Zero performance impact on hot paths. The position computation adds a few nanoseconds when an error actually fires.
- **Spec context:** YAML 1.2.2 is silent on error-position precision. This is a usability defect against the implementation's own design contract (Phase 2 audit requirement), not a spec-conformance defect. The audit classified it as "Lenient (usability)" with explicit taxonomic-stretch documentation.
- **User directive:** "security hardened, fine. Lenient not fine."

## Steps

- [ ] Fix parser-side error positions (classes 1-5)
- [ ] Add `pos` field to `LoadError` variants (class 6)
- [ ] Update `LoadError` consumers in `rlsp-yaml`
- [ ] Add tests for precise positions
- [ ] Update follow-up queue: remove L12-L17 entry
- [ ] Verify all tests pass
- [ ] Mark plan Completed and commit

## Tasks

### Task 1: Fix parser-side error positions (classes 1-5)

Adjust 5 error construction sites to point to the precise offending byte instead of start-of-construct.

- [ ] Class 1 (`%YAML` major-0): compute position of the major digit within the directive line (offset from `dir_pos` past `%YAML ` to the major digit start)
- [ ] Class 2 (`%YAML` u8 overflow): compute position of the overflowing digit (same approach — offset from `dir_pos` to the digit position within `params`)
- [ ] Class 3 (unterminated single-quoted): use the opening `'` position (`open_pos`) instead of `self.current_pos` (EOF). The `open_pos` variable is already in scope at the function level.
- [ ] Class 4 (resolved-tag overflow): grep all call sites of `resolve_tag` and verify each passes the tag's `!` indicator position (not the `---` marker position). Document in the commit message which call sites were checked and whether any needed fixing. Fix any that pass the wrong position.
- [ ] Class 5 (anchor name overflow): offset from `indicator_pos` by the byte length of the name up to the overflow point, so the position points to the first byte beyond `MAX_ANCHOR_NAME_BYTES`
- [ ] Unit tests verifying precise byte positions for each class (compare `err.pos.byte_offset` or `err.pos.column` against expected values)
- [ ] Existing `cargo test -p rlsp-yaml-parser` passes with zero failures
- [ ] `cargo clippy --all-targets` passes with zero warnings
- [ ] `cargo fmt --check` passes

### Task 2: Add position fields to `LoadError` variants (class 6)

Add a `pos: Pos` field to the 5 `LoadError` variants that currently carry no position information, and update all construction sites and consumers.

- [ ] Add `pos: Pos` field to `NestingDepthLimitExceeded`, `AnchorCountLimitExceeded`, `AliasExpansionLimitExceeded`, `CircularAlias`, `UndefinedAlias` in `loader.rs`
- [ ] At each construction site in the loader, capture the span/position of the triggering event (the event that caused the nesting/anchor/alias/expansion to fire) and pass it as the `pos` field
- [ ] Update `rlsp-yaml/src/parser.rs` `LoadError` match arms to use the new `pos` field for diagnostic position reporting (currently these variants fall through to a position-less diagnostic)
- [ ] Update any test code that pattern-matches on these `LoadError` variants to include the new `pos` field
- [ ] Unit tests verifying that each variant carries a position pointing to the triggering node
- [ ] `cargo test -p rlsp-yaml-parser` passes with zero failures
- [ ] `cargo test -p rlsp-yaml` passes with zero failures (LSP consumer updated)
- [ ] `cargo clippy --all-targets` passes with zero warnings
- [ ] `cargo fmt --check` passes
- [ ] Remove error-position imprecision entry (L12-L17) from `project_followup_plans.md`
- [ ] In the conformance doc rewrite entry in `project_followup_plans.md`, update the architectural-findings bullet about position-precision: change "position-precision design contract is implicit (most errors point to start-of-construct rather than offending-byte)" to note that the 6 audited error classes are now fixed but the broader contract remains implicit — the doc rewrite should still add a per-error-class position table
- [ ] Single commit: `fix(rlsp-yaml-parser): add position fields to LoadError variants`

## Decisions

- **Split into two tasks.** Task 1 (parser-side positions) is internal — no public API change. Task 2 (`LoadError` fields) changes a public enum and requires updating downstream consumers in `rlsp-yaml`. Separate tasks enable independent review and isolate the API change.
- **`pos: Pos` field, not `Option<Pos>`.** The triggering event always has a span/position in the event stream. There is no case where a `LoadError` fires without a triggerable event — the event is what caused the limit to be exceeded. Making it non-optional avoids downstream `unwrap_or` noise.
- **Use event span start as the position.** The loader processes `(Event, Span)` pairs. The span's `start` byte offset identifies the source position of the node that triggered the error. This is the most useful position for the user — it points to where the problematic node begins in the source.
- **No feature-log entry for Task 1.** Internal position adjustments.
- **Feature-log entry for Task 2.** Adding `pos` to `LoadError` is a user-visible improvement — LSP diagnostics now carry positions for loader errors that previously had none.
- **No conformance doc updates.** Holistic rewrite deferred.
- **Performance: zero.** Error-path only. No hot-path changes.

## Non-Goals

- **Adding a Warning event variant.** The Phase 2 architectural finding noted the parser has no Warning channel. That's a separate design decision.
- **Full error-position audit.** This fixes the 6 classes identified by Phase 2. Other error classes may have similar imprecision — those are out of scope.
- **Conformance doc rewrite.** Deferred.
