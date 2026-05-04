**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-04

## Goal

Validate directive names and parameters against the `ns-char+` production per YAML 1.2.2 §6.8, closing Phase 1 Lenient findings [84] (`ns-directive-name`) and [85] (`ns-directive-parameter`). Currently, the parser accepts any non-whitespace bytes in directive names and parameters — including C0 controls, DEL, C1 controls, and BOM — without validation. After this fix, directive names and parameters containing non-`ns-char` bytes produce parse errors.

## Context

- **Spec productions:** `[84] ns-directive-name ::= ns-char+` and `[85] ns-directive-parameter ::= ns-char+` (YAML 1.2.2 §6.8). `ns-char` is defined at production [34] as non-break, non-white printable characters: `!`–`~` (x21–x7E), NEL (x85), xA0–xD7FF, xE000–xFFFD, x10000–x10FFFF. Excludes: space, tab, LF, CR, BOM (U+FEFF), C0 controls, DEL (x7F), C1 controls (x80–x84, x86–x9F), surrogates, xFFFE, xFFFF.
- **Phase 1 audit findings:** `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§6.md` entries [84] and [85] — both auditors agreed on Lenient verdict. The conformance doc incorrectly labels these "Conformant."
- **Current code:** `event_iter/directives.rs:88-93` extracts the directive name as everything between `%` and the first whitespace, without `ns-char` validation. Parameters for known directives (`%YAML`, `%TAG`) have their own specific validation that catches some but not all non-`ns-char` bytes. Unknown (reserved) directive parameters have zero validation.
- **Existing predicate:** `chars.rs:207-217` defines `is_ns_char(ch: char) -> bool` — already exists, currently unused in directive parsing.
- **Spec behavior for reserved directives:** §6.8.1 says "YAML 1.2 implementations should ignore unknown directives with an appropriate warning." The "should ignore" applies to the directive's semantic effect, not to accepting malformed body shapes. A directive whose name contains non-`ns-char` bytes does not match the BNF — it is malformed, not merely unknown.
- **User directive:** "security hardened, fine. Lenient not fine." Strict-reject for non-conforming input.
- **Performance:** Directive parsing is cold-path code — typically 0-3 directives per document. Iterating `is_ns_char()` over 3-10 character strings adds negligible cost (nanoseconds). No benchmark concerns.
- **Spec reference:** [YAML 1.2.2 §6.8](https://yaml.org/spec/1.2.2/#68-directives)
- **Existing tests:** `tests/smoke/directives.rs` has comprehensive directive tests (Groups A-N) but no tests for non-`ns-char` bytes in names or parameters.
- **Follow-up queue:** `project_followup_plans.md` has entries for [84]/[85] that will be removed on completion.

## Steps

- [ ] Validate directive names and parameters against `ns-char+` in `directives.rs`
- [ ] Add integration tests for non-`ns-char` rejection in directive names and parameters
- [ ] Update follow-up queue: remove [84]/[85] entry, update Phase 1 Lenient count from 7 to 5
- [ ] Verify all tests pass (`cargo test -p rlsp-yaml-parser`)
- [ ] Mark plan Completed and commit

## Tasks

### Task 1: Enforce `ns-char+` validation on directive names and parameters

Validate directive names and parameters against the existing `is_ns_char` predicate in `chars.rs`. Reject any directive whose name or parameter contains a non-`ns-char` byte with an error that includes the offending character as `U+XXXX`.

- [ ] In `parse_directive()`, after extracting the directive name, validate each char with `is_ns_char()`; if any fails, return an error with the offending codepoint
- [ ] For unknown (reserved) directives, validate each parameter token with `is_ns_char()` before silently ignoring the directive
- [ ] For `%YAML`: the version string is already validated by digit parsing; add `ns-char` pre-validation on the raw parameter string so that `%YAML \x01.2` is rejected with a clear "non-ns-char" error rather than a confusing "malformed version" error
- [ ] For `%TAG`: the handle is validated by `is_valid_tag_handle()`; add `ns-char` pre-validation on the raw parameters string so that non-`ns-char` bytes in the handle/prefix portion are caught with a clear error
- [ ] Error message format: `"directive name contains non-printable character U+XXXX"` or `"directive parameter contains non-printable character U+XXXX"`
- [ ] Integration tests in `tests/smoke/directives.rs` covering:
  - C0 control (e.g., BEL U+0007) in directive name → error
  - DEL (U+007F) in directive name → error
  - BOM (U+FEFF) in directive name → error
  - C0 control in unknown directive parameter → error
  - C1 control (e.g., U+0080) in `%TAG` prefix → error (replaces current partial control-char check)
  - Valid `ns-char` content in unknown directive name and parameters → no error (regression guard)
  - Existing test I-1 (`%FOO bar baz`) continues to pass (valid `ns-char` content)
- [ ] Existing `cargo test -p rlsp-yaml-parser` suite passes with zero failures
- [ ] `cargo clippy --all-targets` passes with zero warnings
- [ ] `cargo fmt --check` passes
- [ ] Remove [84]/[85] entry from `project_followup_plans.md`
- [ ] Update Phase 1 Lenient count in the orchestration pickup note from "7" to "5" and append `; [84]/[85] resolved by directive ns-char validation fix` to the parenthetical that lists prior resolutions
- [ ] Update conformance doc rewrite entry in `project_followup_plans.md`: remove [84] and [85] from the Phase 1 mislabels list (they are now fixed, not mislabeled)
- [ ] Single commit: `fix(rlsp-yaml-parser): validate directive names and parameters against ns-char`

## Decisions

- **Validate name and parameters separately.** The name and each parameter are distinct BNF productions ([84] and [85]). The error message should distinguish which part is invalid so the user knows what to fix.
- **Pre-validate before dispatch.** Validate the raw name and parameter strings for `ns-char` compliance before dispatching to `%YAML`/`%TAG`/unknown handlers. This ensures consistent error messages regardless of directive type and catches malformed bytes that specific handlers might miss (e.g., a DEL in `%YAML\x7F 1.2` would be caught by pre-validation, not by version parsing).
- **Keep existing specific validation.** The `%YAML` version parsing, `%TAG` handle shape check, and `%TAG` prefix control-char check remain — they produce more specific error messages for their respective contexts. The `ns-char` pre-validation is a backstop that catches bytes those specific checks miss.
- **Error includes `U+XXXX` codepoint.** Non-`ns-char` characters may be invisible in editors. Following the pattern established by the c-printable enforcement (commit `666e2f2`), error messages include the codepoint.
- **No feature-log entry.** This is a conformance fix on a cold path (directive parsing), not a user-facing feature change. The commit message and plan file document the change.
- **Performance assessment is a planning artifact, not a task deliverable.** The user asked to check performance implications during clarification. The assessment (negligible — cold-path code, nanoseconds on 3-10 character strings) is documented in Context. No runtime measurement or benchmark verification is needed because directive parsing runs at most a few times per document.

## Non-Goals

- **Tag prefix validation against `ns-uri-char`.** That is [93]/[94]/[95] — a separate follow-up with a different BNF production and validation predicate.
- **Empty suffix in shorthand tags.** That is [99] — a separate follow-up.
- **Conformance doc rewrite.** Deferred to the holistic doc-rewrite plan.
- **`%TAG` comment-after-prefix absorption.** That is Phase 2 entry L4 — separate scope.
