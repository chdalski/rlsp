**Repository:** root
**Status:** Completed (2026-05-04)
**Created:** 2026-05-04

## Goal

Reject literal non-c-printable characters in the YAML input stream per §5.1, with the nb-json exception for quoted scalars per §5.1's JSON-compatibility clause. This is the root-cause fix for Phase 1 Lenient findings [1] c-printable, [27] nb-char, [34] ns-char, and [75] c-nb-comment-text — four entries that all stem from the same gap: the `is_c_printable` predicate exists at `chars.rs:14-26` but is never enforced on literal stream characters. After this fix, the parser's behavior matches the spec: a YAML stream consists only of c-printable characters (plus the broader nb-json set inside quoted scalars).

## Context

- Phase 1 BNF audit (`.ai/audit/2026-04-30-phase1-bnf/summary.md`) found [1] c-printable, [27] nb-char, [34] ns-char, [75] c-nb-comment-text all Lenient — the `is_c_printable` predicate is defined but not applied to literal stream input. The conformance doc labeled them "Conformant"; the audit determined "Lenient."
- The original session clarification established: "security hardened, fine. Lenient not fine." Option α (strict-reject) was chosen over diagnostic-only or lenient-with-doc.
- **Spec §5.1 defines a two-tier character set:**
  - **c-printable** (stream-wide): TAB, LF, CR, x20-x7E, x85 (NEL), xA0-xD7FF, xE000-xFFFD, x10000-x10FFFF. Excludes C0 controls (except TAB/LF/CR), DEL (x7F), C1 controls (except NEL x85), surrogates, xFFFE, xFFFF.
  - **nb-json** (quoted-scalar exception): TAB, x20-x10FFFF. Broader than c-printable — additionally allows DEL, C1 controls, xFFFE, xFFFF. Excludes LF/CR (non-break) and C0 controls except TAB. Per §5.1: "To ensure JSON compatibility, YAML processors must allow all non-C0 characters inside quoted scalars."
- **Surrogates (xD800-xDFFF) can't appear in valid UTF-8** — Rust's `&str` guarantee eliminates them. No enforcement needed.
- **LF and CR are handled by the line splitter** — they terminate lines and don't appear in `Line.content`. No enforcement needed for those.
- **`is_c_printable` already exists** at `chars.rs:14-26`. **`is_nb_json` does not exist** — needs to be added to `chars.rs`.
- **Existing partial enforcement sites** (NUL in trailing comments at `lexer/plain.rs:81`, BOM in document body at `event_iter/step.rs:64-82`) remain — they produce more specific error messages. The new enforcement adds a GENERAL c-printable check that fires for any non-printable the specific checks didn't catch.
- **libfyaml comparison** (from the earlier session research): libfyaml also accepts literal non-printables silently in all contexts — our fix makes us stricter than libfyaml. This is the user's explicit preference: "security hardened, fine. Lenient not fine."
- The fix touches multiple scanner sites (plain, block, comment, single-quoted, double-quoted). The context determines which predicate to apply: c-printable outside quoted scalars, nb-json inside.

## Steps

- [x] Add `is_nb_json` predicate to `chars.rs` with unit tests
- [x] Enforce c-printable on plain scalar content in `lexer/plain.rs`
- [x] Enforce c-printable (as nb-char: c-printable minus line breaks minus BOM) on block scalar content in `lexer/block.rs`
- [x] Enforce c-printable on comment bodies in `lexer/comment.rs`
- [x] Enforce nb-json on single-quoted scalar literal content in `lexer/quoted.rs`
- [x] Enforce nb-json on double-quoted scalar literal content (between escapes) in `lexer/quoted.rs`
- [x] Add integration tests covering each context × representative non-printable characters
- [x] Verify existing yaml-test-suite tests still pass (the suite's valid-YAML fixtures should contain only c-printable characters; invalid-YAML fixtures that contain non-printables should already expect errors)
- [x] Add "Literal Stream Character Validation" entry to `rlsp-yaml-parser/docs/feature-log.md`
- [x] Remove 4 follow-up entries from `project_followup_plans.md` ([1], [27], [34], [75])
- [x] Update the "11 Phase 1 Lenient entries" count in the orchestration pickup note to reflect the removal
- [x] Update the "Non-printable unicode character diagnostic" entry in the rlsp-yaml section of `project_followup_plans.md` — reframe as LSP-layer-only since the parser now enforces c-printable
- [x] Mark plan Completed and commit

## Tasks

### Task 1: Add `is_nb_json` predicate and enforce character-set rules across all scanners

Add the `is_nb_json` predicate to `chars.rs`. Then enforce the spec's two-tier character-set rule at every content-scanning site: c-printable outside quoted scalars, nb-json inside quoted scalars. Each non-c-printable / non-nb-json byte produces a parse error with the offending codepoint and position.

**Completed:** commit `52e0e22` (2026-05-04)

- [x] `is_nb_json` predicate in `chars.rs` with unit tests (positive + negative cases matching the predicate's boundary)
- [x] Plain scalar content: non-c-printable byte → error with codepoint and position
- [x] Block scalar content (literal + folded): non-c-printable byte (excluding line breaks already handled by line splitter) → error
- [x] Comment bodies: non-c-printable byte → error
- [x] Single-quoted literal content: non-nb-json byte → error (allows DEL, C1, FFFE, FFFF per §5.1 JSON-compat)
- [x] Double-quoted literal content (between escapes): non-nb-json byte → error
- [x] Error message includes the offending codepoint as `U+XXXX` and its position (line, column, byte offset)
- [x] Existing specific checks (NUL in trailing comments at `plain.rs:81`, BOM in doc body at `step.rs:64-82`) remain — they fire first with their specific messages; the general c-printable check is a backstop for everything else
- [x] Integration tests in `tests/` covering: C0 control (e.g., U+0007 BEL) in plain scalar, block scalar, comment; DEL (U+007F) in plain scalar (rejected) vs in quoted scalar (accepted per nb-json); C1 control (e.g., U+0080) in plain scalar (rejected) vs in quoted scalar (accepted); U+FFFE in plain scalar (rejected) vs in quoted scalar (accepted)
- [x] yaml-test-suite `cargo test -p rlsp-yaml-parser --test yaml_test_suite` passes (no regressions on the conformance suite)
- [x] `cargo build`, `cargo clippy --all-targets`, `cargo test -p rlsp-yaml-parser` — zero warnings, zero failures
- [x] `cargo fmt --check` passes
- [x] `rlsp-yaml-parser/docs/feature-log.md` has a new "Literal Stream Character Validation" entry documenting: what changed (c-printable enforced on all literal stream input; nb-json exception for quoted scalars), what inputs are now rejected, the security rationale
- [x] `project_followup_plans.md`: 4 entries ([1], [27], [34], [75]) removed; orchestration pickup-note count updated from "11" to "7"; "Non-printable unicode character diagnostic" entry in rlsp-yaml section reframed as LSP-layer-only (parser now enforces c-printable; remaining work is to surface violations as LSP diagnostics)
- [x] Single commit: `fix(rlsp-yaml-parser): reject non-c-printable characters in literal stream input`

## Decisions

- **Strict-reject, not diagnostic-only.** The user's explicit preference ("lenient not fine"). Non-c-printable bytes in literal stream input produce parse errors, not warnings. This matches the spec's normative wording ("YAML streams use only the printable subset").
- **Respect the nb-json exception for quoted scalars.** The spec explicitly requires JSON compatibility: "YAML processors must allow all non-C0 characters inside quoted scalars." This means DEL (U+007F), C1 controls (U+0080-U+009F), U+FFFE, and U+FFFF are ALLOWED inside single-quoted and double-quoted scalars but REJECTED everywhere else. This is a "must" — not optional.
- **Add `is_nb_json` as a new predicate, not inline the check.** The predicate is spec-defined (`[2] nb-json ::= x09 | [x20-x10FFFF]`) and will be referenced by both quoted-scalar scanners. Centralizing it in `chars.rs` matches the existing pattern of spec-production predicates.
- **Error message includes `U+XXXX` codepoint.** Non-printable characters are invisible in editors; the error message must tell the user WHAT the offending character is, not just WHERE it is. Format: `"non-printable character U+XXXX is not allowed in <context>"` where `<context>` is "plain scalar", "block scalar", "comment", etc.
- **Don't change the existing specific checks.** The NUL-in-trailing-comment check at `plain.rs:81` and the BOM-in-document-body check at `step.rs:64-82` produce specific, context-aware error messages. They fire before the general c-printable backstop. Removing them would make error messages less specific.
- **Per-scanner enforcement, not line-level pre-scan.** The nb-json exception is context-dependent (quoted vs non-quoted), so the enforcement must happen at the scanner level where the context is known. A line-level pre-scan would reject nb-json-valid bytes that happen to be inside quoted scalars.
- **Separate byte-level post-scan validation, not integrated into existing `memchr` scan.** Current scanners use SIMD-accelerated `memchr2`/`memchr` to find delimiters in bulk. The c-printable check adds a second pass over the content bytes scanning for non-printables. The non-printable bytes in valid UTF-8 are: C0 controls (bytes `0x00-0x08`, `0x0B-0x0C`, `0x0E-0x1F` — detectable via `b < 0x20 && b != 0x09`), DEL (`0x7F`), C1 controls (UTF-8 sequence `0xC2 0x80-0x9F`), and U+FFFE/FFFF (`0xEF 0xBF 0xBE/0xBF`). A tight byte loop with branch prediction favoring "no match" (valid YAML has no non-printables) is very fast. Expected overhead on normal files: near-zero (scan finds nothing in <1KB content). If benchmarks (`cargo bench`) show a measurable regression on the user's baremetal, a follow-up can integrate the check into the existing `memchr` scan (Approach 3) or use SIMD byte-predicate scanning. The plan does not set a perf acceptance criterion (per `feedback_no_agent_perf_thresholds.md` — perf measurement is the user's job out-of-band on baremetal, not the agent's in Docker).
- **Surrogates are already excluded by Rust's `&str` UTF-8 guarantee.** No enforcement needed for xD800-xDFFF.
- **LF and CR are already handled by the line splitter.** They don't appear in `Line.content`. No enforcement needed in scanners.

## Non-Goals

- **Making non-printables a warning instead of an error.** The parser has no Warning channel (Phase 2 §6.8 architectural finding A1). Even if it did, the user chose strict-reject.
- **Extending to plain/block scalar caps.** The 1 MiB cap fix is separate (committed at `0c19f25`).
- **Fixing the conformance doc.** Deferred to the holistic doc-rewrite plan.
- **Other Lenient findings** (directive validation, tag prefix, empty suffix, flow-line-prefix, signed octal/hex, error positions, double BOM, %TAG comment absorption, verbatim tags). Each has its own follow-up entry.
- **Adding LSP diagnostics for non-printables in `rlsp-yaml/`.** That's downstream — the parser produces errors; the LSP layer surfaces them. No `rlsp-yaml/` changes in this plan.
