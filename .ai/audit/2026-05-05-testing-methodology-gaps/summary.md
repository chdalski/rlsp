# Testing Methodology Gaps — Audit Summary

**Date:** 2026-05-05  
**Scope:** `rlsp-yaml-parser` (deep), `rlsp-yaml` (targeted), `rlsp-fmt` (targeted)  
**Motivation:** Phase 2 §5.2 audit revealed the BOM-less UTF-32 detection gap was missed because
no test covered that row of the encoding detection table and no property test asserted encoding
invariance. This audit surveys whether the same pattern repeats elsewhere.

---

## §5.2 Encoding Detection Table

### GAP-E1: `detect_encoding` UTF-8 BOM minimum 3-byte case not tested
- **Description:** Every BOM test appends at least one content byte. The UTF-8 BOM minimum case
  (exactly 3 bytes: `EF BB BF`) has no dedicated fixture. Rust slice patterns require at least
  the listed bytes, so detection is correct, but the case is undocumented by tests.
- **Bug class:** Detection misfire on truncated BOM-only streams.
- **Severity:** Low.
- **Fix:** Add a fixture in `tests/encoding.rs` for `[0xEF, 0xBB, 0xBF]` (3 bytes, no content)
  asserting it detects as UTF-8 and returns an empty document or appropriate error.

### GAP-E2: No test for UTF-32-BE BOM ordering priority over UTF-16-BE BOM
- **Description:** `[0x00, 0x00, 0xFE, 0xFF]` is UTF-32 BE BOM; `[0xFE, 0xFF, ..]` is UTF-16 BE
  BOM. The spec-row fixture `detect_encoding_covers_all_spec_rows` covers both rows but there is
  no test proving that `[0x00, 0x00, 0xFE, 0xFF, non-zero]` is detected as UTF-32 BE and NOT
  UTF-16 BE. The ordering comment in `detect_encoding()` describes the invariant but no test
  enforces it; a reordering of match arms would silently produce wrong results.
- **Bug class:** Structural regression guard for match-arm ordering.
- **Severity:** Medium.
- **Fix:** Add a fixture in `tests/encoding.rs` asserting `[0x00, 0x00, 0xFE, 0xFF, ...]`
  returns `Encoding::Utf32Be` — not `Encoding::Utf16Be`. Use an `rstest` named case
  `utf32_be_bom_wins_over_utf16_be_prefix` paired with the existing spec-row fixture.

### GAP-E3: BOM-less UTF-16 odd-length (truncated) decode not tested
- **Description:** `TruncatedUtf16` is tested for BOM-prefix inputs (`FF FE` + 1 odd byte). There
  is no test for a BOM-less UTF-16 LE input with odd length (e.g., `[0x41, 0x00, 0x42]` — detected
  as UTF-16 LE by the null-byte heuristic, then fails the even-length check with TruncatedUtf16).
- **Bug class:** Misreported truncation error vs. incorrect decoding for BOM-less odd streams.
- **Severity:** Low.
- **Fix:** Add a fixture in `tests/encoding.rs` with a 3-byte BOM-less UTF-16 LE input
  (e.g., `[0x41, 0x00, 0x42]`) asserting `LoadError::TruncatedUtf16`.

---

## §5 Character-Set Tables (`chars.rs`)

### GAP-C1: `is_c_printable` D7FF/E000 boundary not tested
- **Description:** `is_c_printable` uses `U+A0..=U+D7FF` and `U+E000..=U+FFFD`. The boundary
  codepoints U+D7FF (last BMP codepoint before surrogate range) and U+E000 (first codepoint after
  surrogate range) have no dedicated tests. Rust's `char` excludes surrogates, but a test at the
  boundary documents and locks the intent.
- **Bug class:** Off-by-one in BMP range accepting or rejecting valid codepoints.
- **Severity:** Low.
- **Fix:** Add inline `#[cfg(test)]` fixtures in `chars.rs` (or extend the existing boundary
  tests) for `'\u{D7FF}'` (accepted), `'\u{E000}'` (accepted), and `'\u{FFFE}'` (rejected),
  `'\u{FFFD}'` (accepted).

### GAP-C2: `find_non_c_printable` NEL-adjacent C1 boundary (U+0084 / U+0086) not tested
- **Description:** U+0085 (NEL = 0xC2 0x85) is accepted; U+0084 (0xC2 0x84) and U+0086 (0xC2
  0x86) are rejected. The existing test covers U+0080 but not the codepoints immediately adjacent
  to NEL. An off-by-one in the `b2 != 0x85` guard would silently pass for these adjacent
  codepoints.
- **Bug class:** Off-by-one in NEL exclusion — exact shape of the historical BOM-less UTF-32 miss.
- **Severity:** Medium.
- **Fix:** Add inline `#[cfg(test)]` fixtures in `chars.rs` asserting `find_non_c_printable`
  returns `Some(0)` for inputs containing `\u{0084}` (0xC2 0x84) and `\u{0086}` (0xC2 0x86),
  and returns `None` for a valid `\u{0085}` (NEL) input.

### GAP-C3: `decode_escape` literal-tab `'\t'` arm not tested
- **Description:** `decode_escape` handles `'t' | '\t'` (letter `t` and literal tab character).
  Only `"t"` is tested. The `"\t"` input (literal tab as escape code) is untested — removing the
  `'\t'` arm would leave all existing tests passing.
- **Bug class:** Silent breakage of tab-as-escape-character handling.
- **Severity:** Low.
- **Fix:** Add an inline `#[cfg(test)]` fixture in `chars.rs` calling `decode_escape('\t')`
  (literal tab) and asserting it returns `'\t'`.

### GAP-C4: `is_ns_char` supplementary-plane boundary (U+10000, U+10FFFF) not tested
- **Description:** The range `U+10000..=U+10FFFF` is included in `is_ns_char` via the `matches!`
  macro but has no test. An accidental range truncation would be invisible.
- **Bug class:** ns-char rejecting valid supplementary-plane codepoints.
- **Severity:** Low.
- **Fix:** Add inline `#[cfg(test)]` fixtures asserting `is_ns_char('\u{10000}')` and
  `is_ns_char('\u{10FFFF}')` both return `true`.

---

## §10.2 / §10.3 Integer and Float Regex Tables (`schema.rs`)

### GAP-S1: `is_core_int` unbounded-length octal/hex strings not tested
- **Description:** The implementation accepts any length octal/hex with no upper bound. No test
  confirms behaviour for very long inputs. Not a current correctness bug but a gap if a future
  DoS-related length cap is added without updating tests.
- **Bug class:** Untested length boundary.
- **Severity:** Low.
- **Fix:** Add fixtures in `tests/schema_resolution.rs` (or inline) for a very long octal string
  (e.g., `0o` + 100 digits of `7`) and hex string asserting they resolve to `!!int`.

### GAP-S2: `is_core_float` trailing-dot-with-exponent form `1.e5` not tested
- **Description:** `is_core_decimal_float` allows empty fractional digits (the `1.` form is
  valid). Combined with an exponent, `1.e5` is a valid Core float per spec. Existing tests cover
  `3.14`, `.5`, `1e10` but not `1.e5`. A bug in the empty-frac-with-exponent path would silently
  misclassify `1.e5` as `!!str`.
- **Bug class:** Wrong tag resolution — `1.e5` resolved to `!!str` instead of `!!float`.
- **Severity:** Medium.
- **Fix:** Add an `rstest` named case `trailing_dot_with_exponent` in `tests/schema_resolution.rs`
  asserting `1.e5` resolves to `!!float` under `Schema::Core`.

### GAP-S3: `is_json_float` bare-zero-with-exponent form `0e5` not tested
- **Description:** JSON float `0e5` matches integer-part `0` + exponent `e5`, no fractional part.
  Existing tests cover `0.5`, `-1.5`, `1e10`, `-1.5e-3` but not `0e5`. The `strip_prefix('0')`
  path must continue past the integer-part check into the exponent handler.
- **Bug class:** `0e5` misclassified as not-a-JSON-float, producing `UnresolvedScalar` error.
- **Severity:** Medium.
- **Fix:** Add an `rstest` named case `zero_with_exponent` in `tests/schema_resolution.rs`
  asserting `0e5` resolves to `!!float` under `Schema::Json`.

### GAP-S4: `resolve_collection` schema-independence not directly tested
- **Description:** `resolve_collection` contains `let _ = schema;`. The tests cover all three
  schemas but no test holds everything else constant while varying only `schema` to assert the
  result is identical. A future refactor adding schema-dependent collection resolution would be
  semantically wrong but would not break any current test.
- **Bug class:** Schema accidentally affecting collection resolution.
- **Severity:** Low.
- **Fix:** Add a parameterized test in `tests/schema_resolution.rs` that loads `[1, 2, 3]` under
  all three schemas and asserts the root node tag is `!!seq` in all three cases.

---

## §6.9.1 Tag Resolution Rules (`directive_scope.rs`)

### GAP-T1: `!` primary handle path with URI-invalid percent-decoded suffix not tested
- **Description:** The `resolve_tag` function for `!suffix` with a registered primary `!` handle
  calls `percent_decode` then `validate_resolved_tag`. The secondary `!!` and named `!h!` paths
  are tested for invalid-char rejection (e.g., percent-decoded space). The primary `!` path is
  not tested for the same case.
- **Bug class:** Percent-encoded space (`%20`) in a primary-handle suffix passes through
  `validate_resolved_tag` without rejection.
- **Severity:** Medium.
- **Fix:** Add a fixture in `tests/schema_resolution.rs` (or `tests/smoke/`) loading YAML with
  `%TAG ! prefix:\n---\n!%20foo scalar\n` and asserting it returns a `LoadError` with an
  invalid-tag message.

### GAP-T2: `percent_decode` has no standalone unit tests
- **Description:** `percent_decode` is a private function exercised indirectly via `resolve_tag`.
  Its edge cases (truncated `%X`, invalid hex `%GG`, valid `%2F`) are not directly confirmed.
  There is no test for `!!foo%2Fbar` expanding to `tag:yaml.org,2002:foo/bar`.
- **Bug class:** Silent pass-through of invalid `%HH` sequences if hex-digit check misfires.
- **Severity:** Medium.
- **Fix:** Add inline `#[cfg(test)]` fixtures in `directive_scope.rs` for `percent_decode`
  covering: `%2F` → `/`, `%GG` → `%GG` (pass-through), `%0` → `%0` (truncated, pass-through).
  For the end-to-end case, add a fixture loading `!!foo%2Fbar scalar` and asserting the tag
  expands to `tag:yaml.org,2002:foo/bar`.

---

## §10.1 Default Tag by Kind

### GAP-K1: JSON schema bare `!` on sequence or mapping not tested
- **Description:** `schema_resolution.rs` covers bare `!` on scalars, sequences, and mappings for
  `Schema::Core` (IT-12 to IT-15) and `Schema::Failsafe` (IT-53, IT-54), but not for
  `Schema::Json`. Since `resolve_collection` ignores schema, the result is correct, but the gap
  means a future schema-aware refactor would have no guard.
- **Bug class:** JSON schema silently altering collection tag resolution.
- **Severity:** Low.
- **Fix:** Add fixtures in `tests/schema_resolution.rs` for `Schema::Json` loading `![1, 2]` and
  `!{a: b}` and asserting the tag is the bare `!` local tag (not `!!seq` / `!!map`).

---

## Proptest Candidates

### GAP-P1: No round-trip event preservation property
- **Description:** No proptest asserts that re-parsing a YAML string produced from a first parse's
  events yields the same events. This would catch formatter and serializer bugs that preserve
  structure but corrupt content.
- **Bug class:** Silent content corruption in any serializer built on the parser.
- **Severity:** Medium.
- **Fix:** Add a proptest in `rlsp-yaml-parser/tests/` generating simple scalar/sequence/mapping
  YAML via the `Arbitrary` strategy, parsing to events, rendering back to a canonical form, and
  asserting re-parsing produces identical events.

### GAP-P2: No property for char-predicate subset relationships
- **Description:** `is_ns_tag_char_single` ⊂ `is_ns_uri_char_single` ⊂ `is_ns_char`. A proptest
  `forall ch: is_ns_tag_char_single(ch) → is_ns_uri_char_single(ch)` would catch accidental breaks
  in the hierarchy.
- **Bug class:** A char accepted by the narrower predicate but rejected by the broader one would
  produce a tag that passes scanning but fails URI validation.
- **Severity:** Medium.
- **Fix:** Add a proptest in `rlsp-yaml-parser/src/chars.rs` (inline `#[cfg(test)]`) using
  `proptest::char::any()` asserting the subset relationships between the three predicates.

### GAP-P3: No schema resolution determinism property
- **Description:** No proptest asserts `resolve_scalar` is a pure function. Structurally
  guaranteed by the implementation, but a proptest would catch accidental global state.
- **Bug class:** Non-deterministic tag resolution.
- **Severity:** Low.
- **Fix:** Add a proptest in `tests/schema_resolution.rs` generating random ASCII scalars and
  asserting `resolve_scalar(s, schema)` returns the same result when called twice.

### GAP-P4: Formatter idempotency proptest limited to default options only
- **Description:** `formatter_conformance.rs` tests idempotency against the yaml-test-suite with
  default options only. No proptest covers varied `print_width`, `indent_width`, `single_quote`.
  A formatter that is idempotent at width 80 but not at width 40 passes all current tests.
- **Bug class:** Formatter idempotency failures on non-default settings.
- **Severity:** Medium.
- **Fix:** Add a proptest in `rlsp-yaml/tests/` generating random `YamlFormatOptions`
  (using `Arbitrary`) combined with yaml-test-suite inputs, asserting format applied twice equals
  format applied once.

### GAP-P5: Encoding detection proptest restricted to ASCII strategy
- **Description:** The existing `encoding_choice_invariant_under_parse` proptest uses the strategy
  `[a-z]{1,6}: [0-9]{1,6}\n`. Arbitrary byte prefixes are not covered; only ASCII content.
- **Bug class:** Misdetection of encoding for unusual byte sequences not covered by the strategy.
- **Severity:** Low.
- **Fix:** Extend the proptest strategy to include unicode scalar values (using `proptest::char`)
  converted to each encoding, or add a separate proptest for BOM-less detection of short
  unicode strings.

---

## Differential Testing Candidates

### ~~GAP-D1: No libyaml / PyYAML comparison harness~~ — DOWNGRADED (Not Recommended)

- **Description:** No differential testing compares `parse_events()` output against a reference
  implementation for arbitrary inputs.
- **Reassessment (2026-05-05):** Not recommended. libyaml and PyYAML are YAML 1.1, not 1.2.2 —
  most divergences would be spec-version differences (octal syntax, boolean values, tag
  resolution), not bugs. snakeyaml-engine is 1.2 but has its own known deviations. The parser
  has 5 deliberate Stricter-than-spec decisions that would all appear as false-positive
  "differences." The YAML Test Suite (368/368 pass) already serves as the spec-derived reference
  — it IS the differential test. Signal-to-noise ratio of comparing against another
  implementation would be poor.
- **Severity:** ~~High~~ Low — not cost-effective given existing YAML Test Suite coverage.

### GAP-D2: No cross-schema structural consistency check
- **Description:** Given the same YAML text loaded with all three schemas, the AST structure
  (node types, children, anchor names) should be identical — only the `tag` fields should differ.
  No test asserts this invariant.
- **Bug class:** Schema parameter affecting node structure rather than only tagging.
- **Severity:** Medium.
- **Fix:** Add a parameterized test in `tests/schema_resolution.rs` loading the same input under
  all three schemas and asserting structural equality (excluding tag fields) using a recursive
  comparator.

---

## rlsp-yaml Testing Gaps

### ~~GAP-Y1: I10 round-trip invariant ignores scalar style changes~~ — FALSE POSITIVE

- **Description:** `documents_equivalent` explicitly ignores style. The audit claimed a formatter
  that drops quotes from `"null"` would pass I10 undetected.
- **Verification (2026-05-05):** FALSE POSITIVE. The loader (`loader.rs:1055-1057`) calls
  `resolve_scalar()` and injects the resolved tag directly into the AST `Node::Scalar.tag` field.
  `documents_equivalent()` compares tags via `node_tag_str()` at lines 793-796. So `"null"`
  (quoted → `!!str`) reformatted to `null` (plain → `!!null`) produces different tag values in
  the pre- and post-format ASTs, and I10 already detects this. The audit confused "ignores style"
  with "ignores the semantic effect of style changes."
- **Severity:** ~~High~~ N/A — no gap exists.

### GAP-Y2: No invariant for formatter preserving explicit user-authored tags
- **Description:** There is no isolated invariant testing that a user-authored explicit tag
  (e.g., `!!custom`) on a scalar is preserved through the formatter. I10 covers this transitively
  via `documents_equivalent` tag comparison, but not as a dedicated invariant.
- **Bug class:** Formatter silently stripping or transforming explicit user tags.
- **Severity:** Medium.
- **Fix:** Add a fixture in `rlsp-yaml/tests/formatter_conformance.rs` (or a new
  `tests/formatter_tags.rs`) with inputs containing explicit tags asserting the tag string appears
  unchanged in the formatted output.

### GAP-Y3: Code-action fixture coverage incomplete across categories
- **Description:** Not all code-action categories have fixtures for every combination of
  block/flow context and scalar style. The corpus invariant I3 catches parse-breaking actions but
  not semantic mistakes.
- **Bug class:** Code actions producing structurally valid but semantically wrong YAML.
- **Severity:** Medium.
- **Fix:** Audit existing code-action fixture files; for each action category missing block or
  flow variants, add fixture pairs (input + expected output) that exercise the missing context.

### GAP-Y4: No proptest for code-action idempotency
- **Description:** No property test asserts that applying a code action twice produces the same
  result as applying it once.
- **Bug class:** Non-idempotent code actions producing unexpected changes on subsequent invocations.
- **Severity:** Low.
- **Fix:** Add a proptest in `rlsp-yaml/tests/corpus_invariants.rs` that applies each available
  code action twice to corpus inputs and asserts the second application is a no-op (output equals
  input on the second pass).

---

## rlsp-fmt Testing Gaps

### GAP-F1: `fits()` lookahead has no test for the `FlatAlt` arm
- **Description:** `fits()` has a `Doc::FlatAlt { flat, .. }` arm that uses only the flat variant
  during lookahead. This arm has no dedicated unit test. The `flat_alt_modes` test exercises
  `format()` but does not verify `fits()` specifically for `FlatAlt`.
- **Bug class:** Incorrect flat/break decisions at group boundaries when `FlatAlt` is present.
- **Severity:** Medium.
- **Fix:** Add a unit test in `rlsp-fmt/src/printer.rs` (inline `#[cfg(test)]`) constructing a
  `Doc::Group(Doc::FlatAlt { flat: short_doc, expanded: long_doc })` where `short_doc` fits in
  the column budget, asserting the group is rendered in flat mode and the flat variant is used.

### GAP-F2: `indent_width` tab-mode column accounting not directly tested
- **Description:** `indent_width` with `use_tabs = true` returns `indent` (tab count as 1 column
  each). This is intentional ("tabs are visually ambiguous") but has no test documenting and
  protecting the invariant.
- **Bug class:** Tab-width accounting error causing premature or delayed line breaks.
- **Severity:** Low.
- **Fix:** Add an inline unit test in `printer.rs` calling `indent_width(3, true)` and asserting
  it returns `3` (not `3 * tab_stop`), with a comment explaining the intentional tab-as-1-column
  semantic.

### GAP-F3: `concat([])` empty child list not tested
- **Description:** `Doc::Concat(vec![])` is valid but has no test confirming `format` returns `""`
  and leaves `col` unchanged.
- **Bug class:** Column tracking corruption on empty-concat in a future change.
- **Severity:** Low.
- **Fix:** Add an inline unit test in `printer.rs` asserting `format(Doc::Concat(vec![]), ...)`
  returns an empty string with `col = 0`.

---

## Summary Table

| Gap ID | Area | Severity | Description |
|--------|------|----------|-------------|
| GAP-E1 | §5.2 encoding | Low | UTF-8 BOM minimum 3-byte case not tested |
| GAP-E2 | §5.2 encoding | Medium | UTF-32-BE BOM ordering priority not tested |
| GAP-E3 | §5.2 encoding | Low | BOM-less UTF-16 odd-length truncation not tested |
| GAP-C1 | §5 chars | Low | `is_c_printable` D7FF/E000 boundary not tested |
| GAP-C2 | §5 chars | Medium | `find_non_c_printable` NEL-adjacent C1 boundary not tested |
| GAP-C3 | §5 chars | Low | `decode_escape` literal-tab arm not tested |
| GAP-C4 | §5 chars | Low | `is_ns_char` supplementary-plane boundary not tested |
| GAP-S1 | §10.2/§10.3 schema | Low | `is_core_int` unbounded-length octal/hex not tested |
| GAP-S2 | §10.3 schema | Medium | `is_core_float` `1.e5` form not tested |
| GAP-S3 | §10.2 schema | Medium | `is_json_float` `0e5` form not tested |
| GAP-S4 | §10 schema | Low | `resolve_collection` schema-independence not directly tested |
| GAP-T1 | §6.9.1 tags | Medium | `!` primary handle with URI-invalid percent-decode not tested |
| GAP-T2 | §6.9.1 tags | Medium | `percent_decode` no standalone unit tests |
| GAP-K1 | §10.1 tag-by-kind | Low | JSON schema bare `!` on seq/map not tested |
| GAP-P1 | Proptest | Medium | No round-trip event preservation property |
| GAP-P2 | Proptest | Medium | No char-predicate subset relationship property |
| GAP-P3 | Proptest | Low | No schema resolution determinism property |
| GAP-P4 | Proptest | Medium | Formatter idempotency proptest limited to default options |
| GAP-P5 | Proptest | Low | Encoding detection proptest restricted to ASCII strategy |
| ~~GAP-D1~~ | ~~Differential~~ | ~~High~~ | DOWNGRADED — not cost-effective; YAML Test Suite is the spec-derived reference |
| GAP-D2 | Differential | Medium | No cross-schema structural consistency check |
| ~~GAP-Y1~~ | ~~rlsp-yaml I10~~ | ~~High~~ | FALSE POSITIVE — I10 already catches via loader-injected resolved tags |
| GAP-Y2 | rlsp-yaml | Medium | No formatter preserves explicit user-tag invariant |
| GAP-Y3 | rlsp-yaml | Medium | Code-action fixture coverage incomplete by category |
| GAP-Y4 | rlsp-yaml | Low | No code-action idempotency proptest |
| GAP-F1 | rlsp-fmt | Medium | `fits()` has no test for `FlatAlt` arm |
| GAP-F2 | rlsp-fmt | Low | `indent_width` tab-mode accounting not directly tested |
| GAP-F3 | rlsp-fmt | Low | `concat([])` empty child list not tested |
