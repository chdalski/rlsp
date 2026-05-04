**Repository:** root
**Status:** NotStarted
**Created:** 2026-05-04

## Goal

Stop `%TAG` directives from absorbing trailing comments into the prefix per YAML 1.2.2 §6.8.2, closing Phase 2 Lenient finding L4 (REQ-23). Currently, `%TAG ! ! # primary` parses with `# primary` absorbed into the prefix because the prefix extraction at `directives.rs:230` takes everything after the handle's whitespace without honoring `s-l-comments`. After this fix, the prefix is terminated at the first whitespace that follows the prefix body, trailing `# comment` content is correctly ignored, and non-comment trailing content produces a parse error (consistent with `parse_yaml_directive`'s trailing-content handling).

## Context

- **Spec grammar for `%TAG`:** `[88] ns-tag-directive ::= "TAG" s-separate-in-line c-tag-handle s-separate-in-line ns-tag-prefix`. After the prefix, the line continues with `s-l-comments` (optional whitespace + optional `# comment` + line break). The prefix itself is `ns-tag-prefix` which ends at the first non-`ns-uri-char` byte.
- **Phase 2 audit finding:** `.ai/audit/2026-04-30-phase2-prose/reconciliation-§6.8.md` item 23 (REQ-23) — B found that `%TAG ! ! # primary` absorbs `# primary` into the prefix.
- **Current code:** `directives.rs:230` — `let prefix = params[handle_end..].trim_start_matches([' ', '\t']);` takes all remaining content as the prefix. The `validate_tag_prefix` call at line 268 then validates the entire string including the `# comment` — and `#` IS a valid `ns-uri-char` (it's in the URI character set), so the validation passes. The stored prefix becomes `! # primary` instead of `!`.
- **Fix shape:** After extracting the raw `prefix` string, find its true end — the first whitespace character (space or tab) that follows the prefix body. Everything from that point onward is either whitespace or a comment and should be ignored. This matches how `parse_yaml_directive` already handles trailing comments at lines 126-134 (it finds trailing content after the version and checks it's empty or starts with `#`).
- **Performance:** One `find([' ', '\t'])` call on the prefix string (typically 20-40 chars). Cold path, once per `%TAG` directive. Zero impact.
- **Spec reference:** [YAML 1.2.2 §6.8.2](https://yaml.org/spec/1.2.2/#682-tag-directives)
- **User directive:** "security hardened, fine. Lenient not fine."

## Steps

- [ ] Trim prefix at first trailing whitespace in `parse_tag_directive`
- [ ] Add tests for comment-after-prefix handling
- [ ] Update follow-up queue: remove L4 entry
- [ ] Verify all tests pass
- [ ] Mark plan Completed and commit

## Tasks

### Task 1: Terminate tag prefix at trailing whitespace

After extracting the raw prefix from the `%TAG` line, find the end of the actual prefix body by locating the first whitespace character within the prefix string. Validate that any trailing content is either empty or a comment.

- [ ] In `parse_tag_directive()`, after extracting raw `prefix` at line 230, find the first space/tab within `prefix` — that marks the end of the prefix body. Split into `prefix_body` and `trailing`. Validate that `trailing` (after stripping whitespace) is empty or starts with `#`. If it contains non-comment content, return an error.
- [ ] Use `prefix_body` (not `prefix`) for all downstream validation (length check, `validate_tag_prefix`, storage in `tag_handles`)
- [ ] Error message for non-comment trailing content: `"malformed %TAG directive: unexpected trailing content after prefix"` (consistent with `parse_yaml_directive`'s trailing content error)
- [ ] Integration tests:
  - `%TAG ! ! # comment` → prefix is `!`, comment ignored (the bug case)
  - `%TAG !! tag:yaml.org,2002: # standard` → prefix is `tag:yaml.org,2002:`, comment ignored
  - `%TAG !e! tag:example.com,2026:` → prefix is `tag:example.com,2026:` (no comment — regression guard)
  - `%TAG ! ! garbage` → error (trailing non-comment content)
- [ ] Existing `cargo test -p rlsp-yaml-parser` suite passes with zero failures
- [ ] `cargo clippy --all-targets` passes with zero warnings
- [ ] `cargo fmt --check` passes
- [ ] Remove `%TAG` comment-after-prefix entry from `project_followup_plans.md`
- [ ] Single commit: `fix(rlsp-yaml-parser): stop %TAG prefix from absorbing trailing comments`

## Decisions

- **Trim at whitespace, not at `#`.** The prefix is `ns-tag-prefix` which is a URI-char sequence. URI chars include `#`. The correct terminator is whitespace — `ns-uri-char` does not include space or tab, so the first whitespace after the prefix body marks its end. Scanning for `#` directly would incorrectly split a prefix like `tag:example.com#fragment` which is a valid URI.
- **Validate trailing content.** After the prefix body, the only valid content is optional whitespace + optional `# comment`. Any other content (non-whitespace, non-comment) is malformed — reject with error. This matches `parse_yaml_directive`'s trailing content handling.
- **No feature-log entry.** Cold-path directive parsing conformance fix.
- **No conformance doc updates.** Holistic rewrite deferred.
- **Performance: zero.** One `find` call on a ~30 char string. Cold path.

## Non-Goals

- **`%YAML` trailing comment handling.** Already correctly implemented at `directives.rs:126-134`.
- **Reserved directive trailing content.** Reserved directives are silently ignored — no trailing content parsing needed.
- **Conformance doc rewrite.** Deferred.
