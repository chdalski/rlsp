**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Enforce tag prefix validation against `ns-uri-char` and reject empty shorthand tag suffixes per YAML 1.2.2 §6.9.1, closing Phase 1 Lenient findings [93] (`ns-tag-prefix`), [94] (`c-ns-local-tag-prefix`), [95] (`ns-global-tag-prefix`), and [99] (`c-ns-shorthand-tag`). Currently, tag prefixes in `%TAG` directives accept any `ns-char` byte (too broad — should be `ns-uri-char`), and shorthand tags like `!!` and `!handle!` with empty suffixes are accepted (spec requires `ns-tag-char+`, one-or-more). After this fix, non-`ns-uri-char` bytes in tag prefixes and empty shorthand suffixes produce parse errors.

## Context

- **Spec productions for tag prefix:** `[93] ns-tag-prefix ::= c-ns-local-tag-prefix | ns-global-tag-prefix`, `[94] c-ns-local-tag-prefix ::= "!" ns-uri-char*`, `[95] ns-global-tag-prefix ::= ns-tag-char ns-uri-char*`. The prefix must consist of URI-valid characters per production [39] `ns-uri-char`, plus `%HH` percent-encoded sequences.
- **Spec production for shorthand tags:** `[99] c-ns-shorthand-tag ::= c-tag-handle ns-tag-char+`. The `+` means one-or-more — an empty suffix is not allowed.
- **Phase 1 audit findings:** `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§6.md` entries [93], [94], [95] (Lenient — prefix validated only as `ns-char`, not `ns-uri-char`) and [99] (Lenient — empty suffix explicitly accepted with comment in code).
- **Current tag prefix validation:** `directives.rs:210-222` validates parameters against `is_ns_char` (from the just-completed [84]/[85] fix). This catches control characters and DEL but does not enforce the narrower `ns-uri-char` set. Characters like `{`, `}`, `^`, `\`, backtick are `ns-char` but NOT `ns-uri-char` — they pass the current check but should be rejected in tag prefixes.
- **Current shorthand suffix handling:** `properties.rs:170` has comment `"!! alone with no suffix is valid (empty suffix shorthand)"` — the code at lines 177-181 returns a valid tag when `suffix_bytes == 0`. Similarly, `scan_tag` at line 202 allows `!handle!` with zero-byte suffix. Existing unit tests (`scan_tag_secondary_handle_no_suffix` at line 564, `scan_tag_named_handle_with_empty_suffix` at line 604) verify this lenient behavior.
- **Existing predicates:** `chars.rs:229-255` defines `is_ns_uri_char_single(ch: char)` (production [39]) and `chars.rs:262-284` defines `is_ns_tag_char_single(ch: char)` (production [40]). Both already exist and are used in verbatim tag and suffix scanning.
- **Local vs global prefix distinction:** Local prefixes start with `!` followed by `ns-uri-char*`; global prefixes start with `ns-tag-char` (which excludes `!` and flow indicators) followed by `ns-uri-char*`. The first character requires different validation.
- **Percent-encoding in prefixes:** The spec allows `%HH` sequences in prefixes (they count as valid `ns-uri-char`). The prefix validator must accept `%HH` sequences the same way `scan_tag_suffix` and the verbatim tag scanner already do.
- **Performance:** Tag prefix validation runs once per `%TAG` directive (typically 0-3 per document). Shorthand suffix check runs once per tag property. Both are cold-path — negligible performance impact.
- **Spec reference:** [YAML 1.2.2 §6.9.1](https://yaml.org/spec/1.2.2/#691-node-tags)
- **User directive:** "security hardened, fine. Lenient not fine."
- **Follow-up queue:** `project_followup_plans.md` has entries for [93]/[94]/[95] and [99] that will be removed on completion.

## Steps

- [x] Add `ns-uri-char` validation to tag prefix in `parse_tag_directive`
- [x] Reject empty shorthand tag suffixes in `scan_tag`
- [x] Update existing unit tests that verify lenient empty-suffix behavior
- [x] Add integration tests for prefix validation and empty-suffix rejection
- [x] Update follow-up queue: remove entries, update counts
- [x] Verify all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Enforce `ns-uri-char` on tag prefixes and reject empty shorthand suffixes

**Completed:** commit `918a212` (2026-05-04)

Validate tag prefixes against `ns-uri-char` (with `%HH` support) and reject shorthand tags with empty suffixes.

- [x] In `parse_tag_directive()`, after existing `ns-char` pre-validation and handle/prefix extraction, validate the prefix against `ns-uri-char`: each character must be `is_ns_uri_char_single` or a valid `%HH` percent-encoded sequence; additionally, local prefixes (starting with `!`) must have first char `!` followed by `ns-uri-char*`, and global prefixes must start with `ns-tag-char` (`is_ns_tag_char_single`) followed by `ns-uri-char*`
- [x] Error message format: `"tag prefix contains character not allowed in URI at byte offset N"` (consistent with verbatim tag error at `properties.rs:142`)
- [x] In `scan_tag()`, reject empty suffix for `!!suffix` form: when `suffix_bytes == 0` after `scan_tag_suffix`, return an error instead of accepting `!!` as a valid shorthand tag
- [x] In `scan_tag()`, reject empty suffix for `!handle!suffix` form: when the suffix after the inner `!` has `scan_tag_suffix == 0`, return an error
- [x] Error message for empty suffix: `"shorthand tag requires a non-empty suffix"` 
- [x] Update existing unit tests that assert empty suffix is accepted: `scan_tag_secondary_handle_no_suffix` and `scan_tag_named_handle_with_empty_suffix` — change assertions to expect errors
- [x] Integration tests in `tests/smoke/directives.rs` or `tests/smoke/tags.rs` covering:
  - `%TAG !e! tag:{bad` (curly brace in prefix) → error
  - `%TAG !e! tag:\x60bad` (backtick in prefix) → error
  - `%TAG ! !local` (local prefix starting with `!`) → accepted
  - `%TAG !! tag:yaml.org,2002:` (standard prefix) → accepted (regression guard)
  - `%TAG !e! tag:%41bc` (percent-encoded prefix) → accepted
  - `!! value` (empty suffix on `!!`) → error
  - `!handle! value` with `%TAG !handle! prefix:` → error (empty suffix on named handle)
  - `!!str value` (non-empty suffix) → accepted (regression guard)
- [x] Existing `cargo test -p rlsp-yaml-parser` suite passes with zero failures
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] Remove [93]/[94]/[95] entry from `project_followup_plans.md`
- [x] Remove [99] entry from `project_followup_plans.md`
- [x] Update Phase 1 Lenient count in the orchestration pickup note from "5" to "1" and append `; [93]/[94]/[95]/[99] resolved by tag prefix and shorthand suffix validation fix` to the parenthetical
- [x] Update conformance doc rewrite entry: remove [93], [94], [95], [99] from the Phase 1 mislabels list
- [x] Single commit: `fix(rlsp-yaml-parser): validate tag prefix against ns-uri-char and reject empty shorthand suffix`

## Decisions

- **Bundle [93]/[94]/[95] with [99].** Both are §6.9.1 tag-handling conformance gaps. Fixing them together avoids two separate passes through the same file area and keeps tag validation changes in a single reviewable commit.
- **Validate prefix at the `parse_tag_directive` level, not in `scan_tag`.** The prefix comes from `%TAG` directives and is stored in `directive_scope.tag_handles`. Validating at storage time (in `directives.rs`) catches invalid prefixes before they can be used in tag resolution. This is consistent with the existing handle validation at the same site.
- **Reuse the `%HH` scanning pattern from verbatim tags.** The verbatim tag scanner at `properties.rs:113-133` already validates `%HH` sequences. The prefix validator uses the same byte-level scan pattern: check for `%`, verify two hex digits follow, advance by 3; otherwise validate the character with `is_ns_uri_char_single`.
- **Reject empty suffix with a parse error, not silently.** The code comment at `properties.rs:170` says "empty suffix shorthand is valid" — this was a deliberate implementation choice that contradicts the BNF. The fix changes this to a parse error per the user's "lenient not fine" directive.
- **`!!` as non-specific tag remains valid.** The bare `!` tag (non-specific, production [100]) at `properties.rs:186-190` is correctly handled as a separate code path and is NOT affected by the empty-suffix rejection. `!!` with no suffix is a shorthand tag with empty suffix (rejected); `!` alone is a non-specific tag (accepted). These are distinct productions.
- **No feature-log entry.** Cold-path conformance fix, not user-facing feature change.
- **No consolidation needed for `directives.rs` / `tests/smoke/directives.rs`.** Both the [84]/[85] plan and this plan add small, self-contained validation blocks and test groups to these files. The additions have no shared helpers and no overlapping test patterns — each group tests a distinct BNF production. The files remain well-structured with clear group labels (A–O for tests).
- **Performance assessment is a planning artifact.** Both validations are cold-path. No benchmark concerns.

## Non-Goals

- **Verbatim tag admissibility.** That is Phase 2 entry L5 — separate scope.
- **Verbatim tag separator.** That is Phase 2 entry L6 — separate scope.
- **Post-concatenation tag URI validity.** That is Phase 2 entry L8 — may deduplicate with this fix at user discretion but is a separate production.
- **`%TAG` comment-after-prefix absorption.** That is Phase 2 entry L4 — separate scope.
- **Conformance doc rewrite.** Deferred to the holistic doc-rewrite plan.
