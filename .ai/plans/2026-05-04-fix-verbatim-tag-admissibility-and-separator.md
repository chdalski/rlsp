**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Enforce verbatim tag admissibility and separator requirements per YAML 1.2.2 §6.9.1, closing Phase 2 Lenient findings L5 (verbatim tag admissibility) and L6 (verbatim tag separator). Currently, verbatim tags like `!<$:?>`, `!<:foo>`, and `!<!>` are accepted despite the spec requiring verbatim bodies to "begin with `!` (a local tag) or be a valid URI (a global tag)." Additionally, `!<URI>content` with no whitespace between `>` and content is accepted, while the parallel shorthand form correctly requires separation. After this fix, invalid verbatim tag bodies produce parse errors, and verbatim tags require `s-separate` before node content.

## Context

- **Spec requirement for admissibility (§6.9.1, "Verbatim Tags"):** "Verbatim tags must either begin with a `!` (a local tag) or be a valid URI (a global tag)." Spec Example 6.25 explicitly lists `!<!>`, `!<$:?>`, `!<:foo>` as ERRORS.
- **Spec requirement for separator:** Node properties (including verbatim tags) must be followed by `s-separate(n,c)` before content (§6.7/§6.9).
- **Phase 2 audit findings:** `.ai/audit/2026-04-30-phase2-prose/reconciliation-§6.9.1.md` — Defect 1 (item 3, L5): verbatim body validated only as `ns-uri-char+`, not for admissibility. Defect 2 (item 4, L6): verbatim path advances past `>` without `s-separate` check; shorthand path correctly enforces it at `step.rs:502-516`.
- **Current verbatim code:** `properties.rs:91-164` validates URI characters and length, but does NOT check whether the body is a valid local tag (starts with `!`) or valid URI (starts with a URI scheme letter). After closing `>`, returns `(uri, advance)` — the caller treats the position after `>` as content start with no separation enforcement.
- **Loader conflation:** `loader.rs:1010-1013` has a bare-`!` shortcut that misclassifies verbatim `!<!>` as shorthand non-specific tag → `!!str`. Fixing admissibility at `properties.rs` prevents `!<!>` from reaching the loader.
- **Shorthand separator pattern:** `step.rs:485-516` checks if the first character after a shorthand tag could cause ambiguity and requires whitespace. The verbatim fix mirrors this — after `>`, check that the next character is whitespace, EOF, or a line break.
- **Admissibility check shape:** A local verbatim tag body starts with `!` followed by zero or more URI chars (e.g., `!<! foo>` → body `!foo`). A global verbatim tag body must be a valid URI — pragmatically, it must start with a letter (URI scheme start per RFC 3986). Reject bodies that start with neither `!` nor an ASCII letter.
- **Performance:** Both checks run once per verbatim tag. Verbatim tags are rare in real-world YAML (most tags are shorthand). The admissibility check is one byte comparison; the separator check is one byte comparison. Zero performance impact.
- **Spec reference:** [YAML 1.2.2 §6.9.1](https://yaml.org/spec/1.2.2/#691-node-tags)
- **User directive:** "security hardened, fine. Lenient not fine."

## Steps

- [x] Add admissibility check for verbatim tag bodies in `scan_tag`
- [x] Add separator enforcement after verbatim closing `>` in the caller
- [x] Add integration tests for both fixes
- [x] Update follow-up queue: remove L5 and L6 entries
- [x] Verify all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Enforce verbatim tag admissibility and separator

**Completed:** commit `7ba4c7a` (2026-05-04)

Add an admissibility check on verbatim tag bodies (must start with `!` for local or ASCII letter for global URI) and enforce `s-separate` between verbatim closing `>` and node content.

- [x] In `scan_tag()` verbatim arm, after extracting the URI body and before returning: check that the body starts with `!` (local tag) or an ASCII letter `a-zA-Z` (URI scheme start per RFC 3986). If neither, return an error.
- [x] Additionally reject verbatim body that is exactly `!` (bare exclamation) — `!<!>` is listed in spec Example 6.25 as invalid
- [x] Error message for inadmissible body: `"verbatim tag must begin with '!' (local tag) or be a valid URI (global tag)"`
- [x] For separator: `scan_tag` is called from two sites — `step.rs:474` (block context) and `flow.rs:1270` (flow context). The existing shorthand separator check at `step.rs:485-516` fires after the `scan_tag` match arm in the block path. Extend this check to also cover verbatim tags (currently it only applies to shorthand). In the flow path at `flow.rs:1270`, add the same separator check after the verbatim `scan_tag` result. Error message: `"tag must be separated from node content by whitespace"` (same as shorthand path)
- [x] Integration tests covering:
  - `!<$:?> foo` → error (admissibility: `$` is not `!` or letter)
  - `!<:foo> bar` → error (admissibility: `:` is not `!` or letter)
  - `!<!> foo` → error (admissibility: bare `!` body)
  - `!<tag:yaml.org,2002:str> foo` → accepted (valid global URI)
  - `!<!local> foo` → accepted (valid local tag starting with `!`)
  - `!<tag:yaml.org,2002:str>foo` → error (separator: no whitespace after `>`)
  - `!<tag:yaml.org,2002:str> foo` → accepted (separator: space after `>`)
- [x] Existing `cargo test -p rlsp-yaml-parser` suite passes with zero failures
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] Remove verbatim tag admissibility entry (L5) from `project_followup_plans.md`
- [x] Remove verbatim tag separator entry (L6) from `project_followup_plans.md`
- [x] Single commit: `fix(rlsp-yaml-parser): enforce verbatim tag admissibility and separator`

## Decisions

- **Bundle L5 with L6.** Both are in `properties.rs` verbatim tag handling, same code path. Single commit keeps the changes cohesive.
- **Admissibility uses pragmatic URI-start check, not full RFC 3986 parsing.** The spec says "be a valid URI" but doesn't define a URI validator at the BNF level. A full RFC 3986 parser is out of scope. The pragmatic check — first byte must be `!` (local) or ASCII letter (URI scheme start) — catches all spec Example 6.25 invalid cases (`$`, `:`, bare `!`) while allowing all real-world URIs. This matches the audit's fix sketch.
- **Bare `!` is explicitly rejected.** `!<!>` has body `!` — it starts with `!` so it looks like a local tag, but a local tag body of just `!` is the non-specific tag, and the spec explicitly lists `!<!>` as invalid in Example 6.25. The check: if body starts with `!` AND body length == 1, reject.
- **Separator check mirrors shorthand path.** The error message matches the existing shorthand separator error: `"tag must be separated from node content by whitespace"`. Same user-facing behavior for both tag forms.
- **No feature-log entry.** Cold-path conformance fix on rare verbatim tag handling.
- **No conformance doc updates.** Holistic rewrite deferred.
- **Performance: zero.** Two single-byte comparisons on a rare code path. Verbatim tags are uncommon in real-world YAML.

## Non-Goals

- **Post-concatenation tag URI validity.** That is L8 — separate fix point (resolution-time, not parse-time).
- **Full RFC 3986 URI validation.** Out of scope — the spec doesn't mandate it at the BNF level.
- **Loader bare-`!` shortcut cleanup.** Fixing admissibility prevents `!<!>` from reaching the loader. The loader shortcut is a downstream concern.
- **Conformance doc rewrite.** Deferred.
