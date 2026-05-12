**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Enforce `s-flow-line-prefix(n)` indent-then-separation ordering in quoted scalar continuation lines per YAML 1.2.2 §6.3, closing the last Phase 1 Lenient finding [69]. Currently, continuation lines in multi-line quoted scalars strip all leading whitespace (spaces and tabs) in one pass via `trim_start_matches([' ', '\t'])`, without verifying that the first `n` characters are SPACE characters as required by `s-indent(n)`. A continuation line with leading tabs is accepted as if tabs counted toward indentation. After this fix, continuation lines that do not begin with at least `n` SPACE characters produce parse errors.

## Context

- **Spec production:** `[69] s-flow-line-prefix(n) ::= s-indent(n) s-separate-in-line?` where `[63] s-indent(n) ::= s-space × n` (exactly `n` SPACE characters) and `[66] s-separate-in-line ::= s-white+ | <start-of-line>` (whitespace including tabs). The prefix has two phases: first `n` spaces (indent), then optional whitespace (separation). Tabs are only allowed in the separation phase.
- **Phase 1 audit finding:** `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§6.md` entry [69] — Auditor B correctly identified that `trim_start_matches([' ', '\t'])` does not enforce the indent-then-separation ordering. Lead verdict: Lenient.
- **Current code sites:** Four `trim_start_matches([' ', '\t'])` calls in `lexer/quoted.rs` handle continuation line prefix stripping:
  - Line 141: single-quoted multi-line continuation (in the accumulation loop)
  - Line 344: double-quoted multi-line continuation (in `collect_double_quoted_continuations`)
  - Line 39: single-quoted first-line leading whitespace (initial probe — NOT a continuation line, context is different)
  - Line 253: double-quoted first-line leading whitespace (initial probe — NOT a continuation line, context is different)
- **`Line.indent` field:** `lines.rs:73-76` — pre-computed count of leading SPACE characters only (tabs do not contribute). This field already provides the information needed: if `line.indent >= n`, the line has at least `n` spaces before any non-space character.
- **Indent context:** Quoted scalars receive indent context as `_parent_indent` (single-quoted, line 34) or `block_context_indent` (double-quoted, line 248/322). The `n` in `s-flow-line-prefix(n)` corresponds to the minimum indent the continuation line must meet. For double-quoted scalars, this is already partially enforced at line 350-358 for block context — but only as a "must be indented MORE than n" check, not as a "first n chars must be spaces" check. The flow-line-prefix fix adds the tab-in-indent-position enforcement.
- **Performance:** The fix replaces `trim_start_matches` with a comparison on `line.indent` (pre-computed) followed by a slice at offset `n`. No additional iteration — same or better performance than the current approach.
- **Spec reference:** [YAML 1.2.2 §6.3](https://yaml.org/spec/1.2.2/#63-line-prefixes)
- **User directive:** "security hardened, fine. Lenient not fine."
- **This is the last Phase 1 Lenient finding.** After this fix, all 11 Phase 1 Lenient entries are resolved.

## Steps

- [x] Enforce `s-indent(n)` on continuation lines in single-quoted multi-line scanning
- [x] Enforce `s-indent(n)` on continuation lines in double-quoted multi-line scanning
- [x] Add integration tests for tab-in-indent-position rejection
- [x] Update follow-up queue: remove [69] entry, update Phase 1 Lenient count to 0
- [x] Verify all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Enforce `s-indent(n)` ordering on quoted scalar continuation lines

**Completed:** commit `78dcc62` (2026-05-04)

Replace the indiscriminate `trim_start_matches([' ', '\t'])` on continuation lines with indent-aware prefix stripping: verify `line.indent >= n` (the first `n` characters are spaces), then strip the indent portion and any remaining leading whitespace (the separation portion).

- [x] In the single-quoted multi-line continuation loop (around line 141), replace `trim_start_matches([' ', '\t'])` with indent-aware stripping: check that `line.indent >= required_indent`, reject with error if not, then strip indent + separation
- [x] In `collect_double_quoted_continuations` (around line 344), same replacement: check `line.indent >= required_indent`, reject if not, then strip indent + separation
- [x] Use the already-available indent context for `required_indent`: for single-quoted, the `parent_indent` parameter (currently prefixed `_`, rename to use it); for double-quoted, the `block_context_indent` parameter. In flow context, callers pass `0` (single) or `None` (double) — `s-indent(0)` requires zero spaces, so all leading whitespace is separation and the current behavior is already correct. The enforcement only materially changes behavior for block-context quoted scalars where `n > 0`
- [x] Error message: `"continuation line does not have enough indentation (expected at least N spaces, found M)"` — the message must include the expected and actual indent counts
- [x] Do NOT modify the first-line probes at lines 39 and 253 — those are NOT continuation lines and their `trim_start_matches` is correct (they determine whether the line starts a quoted scalar, before indent context is established)
- [x] Integration tests covering:
  - Multi-line single-quoted scalar with tab-only indent on continuation → error
  - Multi-line double-quoted scalar with tab-only indent on continuation → error
  - Multi-line quoted scalar with spaces + tab on continuation (tab in separation portion, after sufficient spaces) → accepted
  - Multi-line quoted scalar with correct space indent → accepted (regression guard)
  - Blank continuation lines (all whitespace or empty) still produce correct folding behavior
- [x] Existing `cargo test -p rlsp-yaml-parser` suite passes with zero failures
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] yaml-test-suite `cargo test -p rlsp-yaml-parser --test yaml_test_suite` passes
- [x] Remove [69] entry from `project_followup_plans.md`
- [x] Update Phase 1 Lenient count in the orchestration pickup note from "1" to "0" and append `; [69] resolved by flow-line-prefix indent enforcement` to the parenthetical; since count reaches 0, update the orchestration step 2 description to note all Phase 1 Lenient entries are resolved
- [x] Update conformance doc rewrite entry: remove [69] from the Phase 1 mislabels list (it is now fixed)
- [x] Single commit: `fix(rlsp-yaml-parser): enforce s-indent(n) on quoted scalar continuation lines`

## Decisions

- **Fix at the continuation-line level, not at `LineBuffer`.** The `s-flow-line-prefix` production is context-dependent (the `n` parameter varies by scalar). `LineBuffer` does not know `n` — only the scanner does. The fix belongs in the scanner where `n` is available.
- **Use `Line.indent` for the space count.** `Line.indent` is already computed per line and counts only SPACE characters. Comparing `line.indent >= n` is O(1) — no additional scanning needed.
- **Blank continuation lines bypass the indent check.** Per the spec, `l-empty(n,c)` lines (blank or containing only whitespace) are allowed with any indentation — they represent line breaks in the folded content. Only non-blank continuation lines need the `s-indent(n)` enforcement.
- **`required_indent` resolved per call site.** Single-quoted receives `parent_indent: usize` (line 34, currently `_parent_indent` — rename to use it). Double-quoted receives `block_context_indent: Option<usize>` (line 322). Call sites: block-context base.rs passes `plain_parent_indent` for single-quoted and `Some(plain_parent_indent)` for double-quoted; flow-context flow.rs passes `0` for single-quoted and `None` for double-quoted; mapping-key block/mapping.rs passes `0` and `None`. When `n == 0` (flow context), `s-indent(0)` requires zero spaces — all leading whitespace is separation, so the current `trim_start_matches` behavior is already correct. The enforcement only changes behavior for block-context scalars where `n > 0` and a continuation line has tabs in the first `n` positions. For double-quoted with `block_context_indent: None`, skip the indent check (same as current behavior).
- **First-line probes are not affected.** Lines 39 and 253 use `trim_start_matches` to detect whether a line starts a quoted scalar. They run before any indent context is established and are not continuation lines. They are out of scope.
- **No updates to `yaml-spec-conformance.md` entries.** The [67] and [69] entries in the conformance doc cite `trim_start_matches` call sites that this fix replaces. These citations will be stale. Per the established precedent (c-printable, 1 MiB cap, directive ns-char, and tag prefix plans all deferred individual doc updates), the conformance doc rewrite is a holistic follow-up that incorporates all Phase 1 + Phase 2 findings. Individual per-fix patches to the doc create churn the rewrite must reconcile.
- **No feature-log entry.** Internal conformance fix on quoted scalar continuation parsing, not a user-facing feature change.
- **Performance:** Replaces one `trim_start_matches` call with an O(1) comparison + slice. Same or better performance.

## Non-Goals

- **Block scalar indent enforcement.** Block scalars use `s-block-line-prefix(n)` (production [68]), which is just `s-indent(n)` with no separation phase. Block scalar indent is already correctly enforced by the existing indent-counting logic.
- **Flow context indent enforcement beyond quoted scalars.** Flow sequences and mappings have their own indent rules — separate scope.
- **Conformance doc rewrite.** Deferred to the holistic doc-rewrite plan.
