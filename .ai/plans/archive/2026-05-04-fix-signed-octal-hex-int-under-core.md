**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Reject signed octal and signed hex integers under the Core schema per YAML 1.2.2 §10.3, closing Phase 2 Lenient findings L9 (signed octal) and L10 (signed hex). Currently, `-0o10`, `+0o10`, `-0xFF`, `+0xFF` all resolve to `!!int` because `is_core_int` strips the leading sign unconditionally before dispatching to per-base validators. The §10.3 spec table places `[-+]?` only on the decimal int row — octal (`0o [0-7]+`) and hex (`0x [0-9a-fA-F]+`) rows are unsigned. After this fix, signed octal/hex values fall through to `!!str`.

## Context

- **Spec table (§10.3.2 int rows):**
  ```
  | [-+]? [0-9]+         | tag:yaml.org,2002:int (Base 10)
  | 0o [0-7]+            | tag:yaml.org,2002:int (Base 8)
  | 0x [0-9a-fA-F]+      | tag:yaml.org,2002:int (Base 16)
  ```
  The `[-+]?` appears ONLY on the decimal row. Each row is its own regex.
- **Phase 2 audit finding:** `.ai/audit/2026-04-30-phase2-prose/reconciliation-§10.3.md` — Auditor A correctly identified the leniency. Auditor B mis-read the spec table (claimed a "global" sign prefix). Lead re-read the spec table directly and confirmed A's verdict.
- **Current code:** `schema.rs:288-312` — `is_core_int` strips leading `+`/`-` at lines 290-293 before checking `0o`/`0x` prefixes. The sign strip is unconditional, so `-0o10` → `0o10` → matched as octal → `!!int`.
- **Fix shape (from audit):** After stripping the sign, if the body begins with `0o` or `0x`, the sign was invalid for that row → return `false` (falls through to `!!str`).
- **Performance:** `is_core_int` runs once per untagged plain scalar during schema resolution. The fix adds one conditional check (two `starts_with` comparisons) after the existing sign strip — O(1), negligible. This is a cold path: schema resolution runs after parsing, once per scalar. On a typical Kubernetes deployment YAML with ~50 scalars, this adds ~50 comparisons total — unmeasurable.
- **Spec reference:** [YAML 1.2.2 §10.3](https://yaml.org/spec/1.2.2/#103-core-schema)
- **User directive:** "security hardened, fine. Lenient not fine."

## Steps

- [x] Gate sign strip in `is_core_int` to reject signed octal/hex
- [x] Add unit tests for signed octal/hex rejection
- [x] Add integration test via `parse_events` confirming `!!str` resolution
- [x] Update follow-up queue: remove L9/L10 entry
- [x] Verify all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Gate sign strip to decimal-only in `is_core_int`

**Completed:** commit `a2d05a3` (2026-05-04)

After the existing sign strip at `schema.rs:290-293`, check if the stripped body begins with `0o` or `0x`. If the original value had a sign AND the body is octal/hex, return `false` — the sign is not permitted for those rows.

- [x] In `is_core_int`, after sign strip, if a sign was present (`rest != value`) and `rest` starts with `0o` or `0x`, return `false`
- [x] Update the function's doc comment to document the per-row sign constraint
- [x] Inline unit tests in `schema.rs`:
  - `is_core_int("-0o10") == false` (signed octal rejected)
  - `is_core_int("+0o10") == false` (signed octal rejected)
  - `is_core_int("-0xFF") == false` (signed hex rejected)
  - `is_core_int("+0xFF") == false` (signed hex rejected)
  - `is_core_int("0o10") == true` (unsigned octal still accepted — regression guard)
  - `is_core_int("0xFF") == true` (unsigned hex still accepted — regression guard)
  - `is_core_int("-42") == true` (signed decimal still accepted — regression guard)
  - `is_core_int("+42") == true` (signed decimal still accepted — regression guard)
- [x] Integration test in `tests/smoke/` confirming `-0o10` and `+0xFF` resolve to `!!str` (not `!!int`) through `parse_events` or `load`
- [x] Existing `cargo test -p rlsp-yaml-parser` suite passes with zero failures
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] Remove signed octal/hex entry from `project_followup_plans.md`
- [x] Single commit: `fix(rlsp-yaml-parser): reject signed octal and hex integers under Core schema`

## Decisions

- **Gate after sign strip, not before.** The cleanest fix is to check `rest` (post-strip) for `0o`/`0x` when `rest != value` (a sign was stripped). This preserves the existing control flow and adds one branch.
- **Return `false`, not error.** Schema resolution is not validation — unmatched values fall through to `!!str`. Returning `false` from `is_core_int` causes the dispatch at `schema.rs:217` to try `is_core_float`, which also won't match, so the value resolves to `!!str`. This is the correct behavior for an unrecognized scalar.
- **No feature-log entry.** Schema resolution conformance fix, not a user-facing feature. The commit message documents the change.
- **No updates to `yaml-spec-conformance.md`.** Per established precedent — holistic rewrite deferred.
- **Performance: negligible.** One extra conditional per scalar after sign strip. O(1). Cold path.

## Non-Goals

- **Leading-zero decimal rejection.** That is the Stricter-than-spec design decision (REQ-7) — separate user decision.
- **JSON schema signed int handling.** JSON schema has its own int regex; this fix is Core-only.
- **Conformance doc rewrite.** Deferred.
