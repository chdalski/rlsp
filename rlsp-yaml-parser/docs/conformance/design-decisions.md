# Design Decisions and Stricter-than-Spec Rationales

This file documents all Stricter-than-spec entries across Phase 1 (BNF) and Phase 2
(prose), user design decisions confirmed post-Phase-2, and spec errata observed during
the audit. It also holds the §7 BNF-trace analysis cross-reference and the
"should is non-mandatory" precedent.

---

## Stricter-than-spec Entries

### Phase 1: [59] / [60] / [61] — Trojan Source mitigation via hex numeric escapes

**Productions:** `ns-esc-8-bit` / `ns-esc-16-bit` / `ns-esc-32-bit`

**Spec behavior:** the BNF for `\x`, `\u`, `\U` escapes specifies only the hex-digit
count; it places no restriction on the decoded codepoint beyond it being a valid Unicode
scalar value.

**Parser behavior:** stricter. After decoding the hex digits, `decode_escape()` in
`src/chars.rs` rejects the decoded character if it falls outside `is_c_printable()` or
is a bidirectional control character (U+202A–U+202E, U+2066–U+2069, U+200F, U+061C).
Named escapes (`\0`, `\a`, `\n`, etc.) are exempt — only hex numeric escapes face this
check.

**Rationale:** Trojan Source attacks (CVE-2021-42574) exploit bidirectional control
characters to make malicious code appear visually innocuous. A YAML document read by an
LSP server may contain user-supplied strings that are then embedded in generated source
code or diagnostics. Rejecting bidi-control characters via numeric escapes at parse time
prevents that class of injection at the protocol boundary. The exemption for named
escapes follows the principle that explicit intent (`\n` = newline, `\0` = null) is less
ambiguous than opaque codepoints produced via `\u202E`.

**User decision (Phase 1):** keep as-is.

**Enforcement site:** `decode_escape()` in `src/chars.rs` — `is_c_printable()` predicate.

---

### Phase 1: [86] — Major-version-0 rejection

**Production:** `ns-yaml-directive`

**Spec behavior:** a 1.2 YAML processor must reject directives with a major version
higher than 1. The spec places no requirement on `major == 0`.

**Parser behavior:** stricter. `parse_yaml_directive()` in `src/event_iter/directives.rs`
checks `if major != 1` — this rejects both `major == 0` and `major >= 2`.

**Behavioral refinement (Phase 2):** only `major == 0` is blocked beyond what the spec
requires. `%YAML 1.0` is accepted — the check gates on `major != 1`, so minor = 0 is
not independently rejected. `%YAML 1.300` is rejected via the u8 digit overflow check
([87]), not this gate.

**Rationale:** no defined YAML 0.x version exists; accepting `%YAML 0.5` would be
meaningless at best and misleading at worst. The defensive rejection costs nothing —
there is no valid document that needs it.

**User decision (Phase 1):** keep as-is.

**Enforcement site:** `parse_yaml_directive()` in `src/event_iter/directives.rs`.

---

### Phase 1: [87] — Minor-version u8 digit overflow

**Production:** `ns-yaml-version`

**Spec behavior:** the BNF `ns-dec-digit+` admits any number of decimal digits for the
major and minor version components.

**Parser behavior:** stricter. `parse_yaml_directive()` uses `parse::<u8>()` for both
major and minor; values outside [0, 255] produce a parse error. `%YAML 1.300` is
rejected.

**Rationale:** no realistic YAML version exceeds 255 for either component. The `u8`
parse is a pragmatic limit that avoids unbounded version-number inputs. The cost is
negligible: no real YAML document uses a minor version above 255.

**User decision (Phase 1):** keep as-is.

**Enforcement site:** `parse::<u8>()` calls in `parse_yaml_directive()` in
`src/event_iter/directives.rs`.

---

### Phase 2: S3 — `MAX_DIRECTIVES_PER_DOC = 64`

**Spec behavior:** YAML 1.2.2 §6.8 has no per-document directive count limit. The spec
allows an arbitrary number of `%TAG` and `%YAML` directives before a document marker.

**Parser behavior:** stricter. `src/event_iter/directives.rs` hard-rejects when
`directive_count >= MAX_DIRECTIVES_PER_DOC` (64). The 65th directive in a single
document produces a parse error regardless of its type.

**Rationale:** DoS protection. A pathological document with thousands of `%TAG` directives
would force the parser to allocate and track an unbounded number of handle-to-prefix
mappings. The 64-entry limit bounds the per-document allocation at the parser level,
before the loader sees the events. 64 directives covers every realistic use case (a
document needing more than a handful of `%TAG` declarations is rare in practice).

**User decision (2026-05-05):** keep as-is. The DoS protection rationale is accepted;
the limit is not configurable via `LoaderOptions`.

**Enforcement site:** `src/event_iter/directives.rs` — `directive_count >= MAX_DIRECTIVES_PER_DOC`.

Constant definition: `src/limits.rs` — `pub const MAX_DIRECTIVES_PER_DOC: usize = 64`.

---

### Phase 2: S4 — Core schema leading-zero decimal rejection

**Spec behavior:** YAML 1.2.2 §10.3 Core schema int regex: `[-+]? [0-9]+`. The regex
literally permits leading zeros: `007`, `01`, `0123` all match and would resolve to
`!!int`.

**Parser behavior:** stricter. `is_core_int()` in `src/schema.rs` rejects strings whose
stripped body begins with `0` and has length > 1 — these resolve to `!!str` instead.

**Rationale:** two independent motivations:

1. **YAML 1.1 octal confusion.** Many YAML 1.1 producers wrote `007` intending octal
   (YAML 1.1's `[0-9]+` octal notation). A YAML 1.2 parser that silently resolves `007`
   to integer 7 would silently misinterpret those documents. Rejecting leading-zero
   decimals forces the user to fix the input rather than receiving a silent wrong answer.

2. **LSP diagnostic enablement.** An LSP server consuming the parsed AST can surface a
   precise "leading-zero decimal — did you mean octal?" diagnostic when the parser
   rejects the token and returns it as `!!str`. This is more informative than silently
   accepting `007` as integer 7.

**User decision (2026-05-05):** keep as-is. Both motivations accepted; the rejection
is intentional and should not be relaxed to match spec.

**Enforcement site:** `is_core_int()` in `src/schema.rs` — the leading-zero check after
sign stripping.

---

## "Should is non-mandatory" Precedent

**Established at:** Phase 1, `[83] ns-reserved-directive` (see
[bnf-§6.md](bnf-§6.md)).

**Rule:** when the YAML spec uses "should" (RFC 2119 meaning: recommended, not mandatory),
the parser's silence is Strict-conformant. The implementation need not emit a warning or
take the recommended action.

**Application across Phase 2:** this precedent settled three §6.8 cases where the spec
says "should … with appropriate warning":

- §6.8.1: `%YAML 1.3` processed — "should be processed with an appropriate warning." The
  parser stores `version: (1, 3)` in `DocumentStart` and parses with 1.2 rules. Consumers
  may emit their own warning by reading `DocumentStart.version`.
- §6.8.1: `%YAML 1.1` processed — "should be processed with appropriate adjustment." The
  parser stores `version: (1, 1)` and applies 1.2 rules uniformly. Same consumer opt-in
  as above.
- §6.8: Unknown directive ignored — "should ignore unknown directives with an appropriate
  warning." Silent ignore per [83] precedent.

**Architectural note:** the parser has no `Event::Warning` variant or warning collector.
The event stream is `Result<(Event, Span), Error>` — success or error, no middle path.
The three "should warn" sites above are the most visible consequence. A future design
enhancement could add a warning side-channel (e.g., a `warnings: Vec<Warning>` field on
the `ParseEventIter`), which would benefit LSP consumers in particular.

---

## BNF-trace Analysis: §7.3.x vs §7.4.2 Implicit Keys

The full analysis is in `[110] nb-double-text(n,c)` in [bnf-§7.md](bnf-§7.md). Summary
for cross-reference:

The spec prose in §7.3.1, §7.3.2, §7.3.3 says "scalars are restricted to a single line
when contained inside an implicit key." This is a terminology trap — the formal meaning
is encoded in the BNF context labels, not the prose:

- `BLOCK-KEY` and `FLOW-KEY` are the formal "implicit key" contexts. Scalars in these
  contexts are restricted to a single line.
- `FLOW-IN` means "inside a flow collection but not formally an implicit key." Multi-line
  scalars are permitted in `FLOW-IN`.

The spec cross-reference at §7.4.2 `ns-flow-pair` uses `FLOW-KEY` for JSON-style keys,
confirming the single-line restriction applies to JSON-style implicit keys. The parser
applies the context label correctly: `try_consume_double_quoted()` in `lexer/quoted.rs`
enforces single-line in `BLOCK-KEY` and `FLOW-KEY` contexts; multi-line is permitted in
`FLOW-OUT` / `FLOW-IN`. This matches the BNF, not the potentially-misleading prose
summary.

---

## Spec Errata Observed

### §10.2 JSON Schema: `-0` worked example contradicts int regex

**Spec line 6578 (normative int regex):** `0 | -? [1-9] [0-9]*`

- The `0` alternative: no sign. `-0` does NOT match.
- The `-? [1-9] [0-9]*` alternative: requires integer part beginning with `[1-9]`.

**Spec line 6601 (worked example):** shows `-0` resolving to integer `0`.

These are internally inconsistent. The YAML spec convention is that BNF/regex is the
formal normative rule; worked examples are illustrative. The parser follows the literal
regex: `is_json_int()` in `src/schema.rs` rejects `-0` (the `0` arm carries no sign);
`is_json_float()` accepts `-0` via the float regex's `-? ( 0 )` arm.

Under the Core schema (§10.3), the int regex is `[-+]? [0-9]+`, which permits `-0`. So
`-0` resolves to `!!int` under Core and `!!float` under JSON — both behaviors are correct
for their respective schemas per the normative regex tables.

**Parser implementation choice:** follow the literal regex, not the worked example. This
matches de-facto behavior among mature JSON-schema YAML parsers (libyaml, PyYAML, etc.).

**Enforcement site:** `is_json_int()` in `src/schema.rs`.
