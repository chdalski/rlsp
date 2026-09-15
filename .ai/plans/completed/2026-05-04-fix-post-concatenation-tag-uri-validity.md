**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Validate resolved tag URIs after handle+suffix concatenation per YAML 1.2.2 §6.9.1, closing Phase 2 Lenient finding L8 (REQ-17). Currently, `resolve_tag` in `directive_scope.rs` concatenates prefix + decoded suffix without re-validating that the result is a well-formed URI. A malformed suffix that passes `ns-tag-char` validation can produce a resolved tag containing characters not allowed in URIs (e.g., through `%HH` decoding that produces non-`ns-uri-char` bytes). After this fix, resolved tags from handle+suffix concatenation are validated against `ns-uri-char`.

## Context

- **Spec requirement (§6.9.1):** "The tag must be a valid URI." After handle expansion, the resolved tag (prefix + decoded suffix) must be a valid URI.
- **Phase 2 audit finding:** `.ai/audit/2026-04-30-phase2-prose/reconciliation-§6.9.1.md` item 20 (REQ-17) — B found that post-concatenation results are not re-validated. Distinct from Phase 1 [93]/[94]/[95] which addressed prefix-side validation at registration time.
- **Current code:** `directive_scope.rs:79-155` — `resolve_tag` concatenates `prefix + percent_decode(suffix)` at three sites (lines 99, 117, 140) and checks only the resolved length (`MAX_RESOLVED_TAG_LEN`). No URI character validation on the result.
- **`percent_decode` function:** Decodes `%HH` sequences in the suffix into literal bytes. A suffix like `%00` decodes to NUL, which is not a valid URI character. The decoded suffix passes `ns-tag-char` validation at scan time (because `%HH` is valid `ns-tag-char` syntax), but after decoding, the literal byte may not be `ns-uri-char`.
- **Interaction with Phase 1 fix:** Phase 1 [93]/[94]/[95] (now fixed) validates the prefix at `%TAG` registration time against `ns-uri-char`. The suffix is validated at scan time against `ns-tag-char` (a subset of `ns-uri-char` minus `!`, `,`, `[`, `]`, `{`, `}`). The gap is: (1) `%HH` decoding can introduce non-URI bytes, and (2) the concatenation result is not checked as a whole.
- **Fix shape:** After concatenation at each of the three sites in `resolve_tag`, validate the resolved string against `ns-uri-char` (reuse the same byte-level scan pattern used in `validate_tag_prefix` and the verbatim tag scanner). If invalid, return an error.
- **Performance:** Runs once per shorthand tag that undergoes handle expansion. Most YAML has 0-5 tagged nodes. Validation iterates the resolved string (~30-60 chars). Cold path. Zero impact.
- **Spec reference:** [YAML 1.2.2 §6.9.1](https://yaml.org/spec/1.2.2/#691-node-tags)
- **User directive:** "security hardened, fine. Lenient not fine."

## Steps

- [x] Add post-concatenation URI validation in `resolve_tag`
- [x] Add tests for invalid resolved tags
- [x] Update follow-up queue: remove L8 entry
- [x] Verify all tests pass
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Validate resolved tag URI after concatenation

**Completed:** commit `a4be7f5` (2026-05-04)

After each prefix+suffix concatenation in `resolve_tag`, validate the resolved string against `ns-uri-char` (with `%HH` support). Reuse the existing validation pattern.

- [x] Extract a reusable `validate_uri_chars(s: &str) -> Result<(), usize>` helper (or reuse `validate_tag_prefix` if it fits — check whether the local-vs-global first-char distinction applies to resolved tags). The validation must accept `ns-uri-char` singles and `%HH` sequences, same as `validate_tag_prefix`.
- [x] Call the validation after each of the three concatenation sites in `resolve_tag` (lines 99, 117, 140 in `directive_scope.rs`). If validation fails, return an error with the resolved tag and the byte offset of the invalid character.
- [x] Error message: `"resolved tag contains character not allowed in URI at byte offset N"`
- [x] Integration tests:
  - `!!str` with default prefix → resolves to `tag:yaml.org,2002:str` → accepted (regression guard)
  - Custom `%TAG` with suffix containing `%00` → resolved tag contains NUL → error
  - Custom `%TAG` with valid suffix → accepted (regression guard)
- [x] Existing `cargo test -p rlsp-yaml-parser` passes with zero failures
- [x] `cargo clippy --all-targets` passes with zero warnings
- [x] `cargo fmt --check` passes
- [x] Remove post-concatenation tag URI validity entry (L8) from `project_followup_plans.md`
- [x] Single commit: `fix(rlsp-yaml-parser): validate resolved tag URI after handle+suffix concatenation`

## Decisions

- **Validate the resolved string, not just the suffix.** The prefix is already validated at registration time (Phase 1 fix), and the suffix is validated at scan time. But `%HH` decoding can introduce bytes that were valid in encoded form but not in decoded form. Validating the full resolved string catches this edge case.
- **Reuse existing validation pattern.** The `validate_tag_prefix` function or a shared helper provides the same byte-level URI-char scan. No new validation logic needed.
- **Error at resolution time, not at scan time.** The suffix `%00` is syntactically valid `ns-tag-char` at scan time (percent-encoding is valid syntax). The invalidity only manifests after decoding. Resolution time is the correct enforcement point.
- **No feature-log entry.** Cold-path conformance fix.
- **No conformance doc updates.** Holistic rewrite deferred.
- **Performance: zero.** One URI-char scan per resolved tag. Cold path, rare operation.

## Non-Goals

- **Re-validating verbatim tags.** Verbatim tags are already validated in `scan_tag` (admissibility + URI chars). No concatenation occurs.
- **Changing `percent_decode` behavior.** The decoder correctly decodes `%HH` → literal bytes. The fix validates the output, not the decoder.
- **Conformance doc rewrite.** Deferred.
