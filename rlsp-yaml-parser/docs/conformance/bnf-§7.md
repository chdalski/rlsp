# BNF Conformance — §7 Flow Style Productions

Source: `.ai/audit/2026-04-30-phase1-bnf/reconciliation-§7.md` (58 entries)

**Verdict tally (post-fix):** Strict-conformant: 57, Stricter-than-spec: 0, Not-applicable: 1

§7's clean tally reflects that the parser's flow-style implementation is mature and well-tested. The §7.3.x-vs-§7.4.2 BNF-trace analysis (see [110] below) is the key interpretive decision for this chapter.

---

### [104] c-ns-alias-node

BNF: `c-ns-alias-node ::= c-alias ns-anchor-name`

- **Verdict:** Strict-conformant
- **Spec (§7.1):** "An alias node is denoted by the `*` indicator. The alias refers to the most recent preceding node having the same anchor."
- **Implementation:** Flow context: `*` consumed, `scan_anchor_name()` called, `Event::Alias` pushed in `event_iter/flow.rs`; tag/anchor on alias rejected as errors. Block context: alias scanning via `scan_anchor_name()` in `event_iter/properties.rs`. `resolve_alias()` in `loader.rs` handles undefined-alias errors.
- **Tests:** `rlsp-yaml-parser/tests/smoke/anchors_and_aliases.rs`; `tests/yaml-test-suite/src/3GZX.yaml` (Spec Example 7.1. Alias Nodes)

### [105] e-scalar

BNF: `e-scalar ::= ""`

- **Verdict:** Strict-conformant
- **Spec (§7.2):** "YAML allows the node content to be omitted in many cases. Nodes with empty content are interpreted as if they were plain scalars with an empty value."
- **Implementation:** `empty_scalar_event()` in `lib.rs` builds `Event::Scalar { value: Cow::Borrowed(""), style: Plain, … }`; emitted at all empty-node sites in `event_iter/flow.rs`, `event_iter/block/mapping.rs`, `event_iter/block/sequence.rs`, and `event_iter/base.rs`
- **Tests:** `tests/yaml-test-suite/src/WZ62.yaml` (Spec Example 7.2. Empty Content); `tests/yaml-test-suite/src/FRK4.yaml` (Spec Example 7.3. Completely Empty Flow Nodes)

### [106] e-node

BNF: `e-node ::= e-scalar`

- **Verdict:** Strict-conformant
- **Spec (§7.2):** "Both the node's properties and node content are optional. This allows for a completely empty node."
- **Implementation:** `empty_scalar_event()` in `lib.rs` — `e-node` collapses to `e-scalar`; emitted at all sites listed for [105]
- **Tests:** `tests/yaml-test-suite/src/FRK4.yaml` (Spec Example 7.3. Completely Empty Flow Nodes)

### [107] nb-double-char

BNF: `nb-double-char ::= c-ns-esc-char | ( nb-json - c-escape - c-double-quote )`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "The double-quoted style is specified by surrounding `"` indicators."
- **Implementation:** `try_consume_double_quoted()` in `lexer/quoted.rs` — `memchr2` scans for `\` and `"`; escape sequences decoded via `decode_escape()`; all `nb-json` chars other than `\` and `"` pass through unmodified
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/7A4E.yaml` (Spec Example 7.6. Double Quoted Lines)
- **Rationale:** The hex-escape printability gate and bidi-control rejection live at [59]/[60]/[61], not at [107]. Per the symmetric reconciliation principle (attribute the verdict to the production where the rule is enforced), [107] is Strict-conformant.

### [108] ns-double-char

BNF: `ns-double-char ::= nb-double-char - s-white`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "The double-quoted style is specified by surrounding `"` indicators."
- **Implementation:** Whitespace trimming of leading/trailing spaces on each continuation line in `lexer/quoted.rs` implements `ns-double-char` in multi-line context
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/7A4E.yaml`

### [109] c-double-quoted(n,c)

BNF: `c-double-quoted(n,c) ::= c-double-quote nb-double-text(n,c) c-double-quote`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "The double-quoted style is specified by surrounding `"` indicators."
- **Implementation:** `try_consume_double_quoted()` in `lexer/quoted.rs` — opening `"` detected, body consumed, closing `"` required
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/LQZ7.yaml` (Spec Example 7.4. Double Quoted Implicit Keys)

### [110] nb-double-text(n,c)

BNF: `nb-double-text(n,FLOW-OUT) ::= nb-double-multi-line(n)` / `nb-double-text(n,FLOW-IN) ::= nb-double-multi-line(n)` / `nb-double-text(n,BLOCK-KEY) ::= nb-double-one-line` / `nb-double-text(n,FLOW-KEY) ::= nb-double-one-line`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "Double-quoted scalars are restricted to a single line when contained inside an implicit key."
- **Implementation:** `try_consume_double_quoted()` in `lexer/quoted.rs` — multi-line path taken when a closing `"` is not found on the first line; `event_iter/flow.rs` enforces single-line in BLOCK-KEY and FLOW-KEY contexts
- **Tests:** `tests/yaml-test-suite/src/LQZ7.yaml` (single-line); `tests/yaml-test-suite/src/7A4E.yaml` (multi-line)

#### BNF-trace analysis: the §7.3.x-vs-§7.4.2 terminology trap

The §7.3.1, §7.3.2, §7.3.3 prose informally says "scalars are restricted to a single line when contained inside an implicit key." The precise meaning is encoded in the BNF context labels — `BLOCK-KEY` and `FLOW-KEY` are the formal "implicit key" contexts; `FLOW-IN` is "inside a flow collection but not formally an implicit key." The terms differ.

**Three places where implicit keys appear in the spec:**

1. **Block mapping implicit keys** (§8.2.2, [193] `ns-s-block-map-implicit-key`):
   ```
   ns-s-block-map-implicit-key ::=
       c-s-implicit-json-key(BLOCK-KEY)
     | ns-s-implicit-yaml-key(BLOCK-KEY)
   ```
   Hardcoded `BLOCK-KEY` → `nb-double-text(n,BLOCK-KEY) ::= nb-double-one-line` → **one-line**.

2. **Flow sequence single-pair compact form** (§7.4.1, [152] `ns-flow-pair-yaml-key-entry`):
   ```
   ns-flow-pair-yaml-key-entry(n,c) ::=
     ns-s-implicit-yaml-key(FLOW-KEY)   # hardcoded FLOW-KEY, NOT parent c
     c-ns-flow-map-separate-value(n,c)
   ```
   Hardcoded `FLOW-KEY` → **one-line**, regardless of outer context.

3. **Flow mapping entry keys** (§7.4.2, [145] `ns-flow-map-yaml-key-entry`):
   ```
   ns-flow-map-yaml-key-entry(n,c) ::=
     ns-flow-yaml-node(n,c)             # uses PARENT context c, NOT hardcoded FLOW-KEY
     ...
   ```
   Uses parent context `c` flowing in from `c-flow-mapping(n,c)`:
   ```
   c-flow-mapping(n,c) ::=
     c-mapping-start
     s-separate(n,c)?
     ns-s-flow-map-entries(n,in-flow(c))?    # in-flow(c) maps the context
     c-mapping-end
   ```
   And `in-flow(c)` (§7.4):
   ```
   in-flow(n,FLOW-OUT)  ::= ns-s-flow-seq-entries(n,FLOW-IN)
   in-flow(n,FLOW-IN)   ::= ns-s-flow-seq-entries(n,FLOW-IN)
   in-flow(n,BLOCK-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)
   in-flow(n,FLOW-KEY)  ::= ns-s-flow-seq-entries(n,FLOW-KEY)
   ```

   So inside `{ key: value }`:
   - At top level (outer `c=FLOW-OUT` / `FLOW-IN`): entries get `FLOW-IN` context → key parses as `nb-double-text(n,FLOW-IN) ::= nb-double-multi-line(n)`. **Multi-line allowed.**
   - Inside a block-key or flow-key context (outer `c=BLOCK-KEY` / `FLOW-KEY`): entries get `FLOW-KEY` → key parses as `nb-double-text(n,FLOW-KEY) ::= nb-double-one-line`. **One-line.**

**Why the asymmetry is deliberate.** Flow-sequence-pair compact form uses the named `ns-s-implicit-yaml-key(FLOW-KEY)` production — a formal "implicit key" with hardcoded one-line constraint. Flow-mapping entry keys use `ns-flow-yaml-node(n,c)` — a regular flow node with parent context, NOT a formal "implicit key." The colloquial reading of "the key in `{ a: b }` is an implicit key" is correct in everyday language but not in spec-grammar terminology.

**Why nested cases still work.** A flow mapping nested inside a block-mapping implicit key (`{ a: b }: value`) is constrained at the OUTER level by `ns-s-implicit-yaml-key(BLOCK-KEY)`'s single-line rule. The entire `{ a: b }` must fit on one line; inner keys are naturally one-line by the outer constraint.

**Concrete examples illustrating the verdict.**

Spec-conformant (multi-line implicit key in top-level flow mapping accepted):
```yaml
{
  long
  key: value
}
```
At top level, c=FLOW-OUT → entries=FLOW-IN → key=`ns-plain(n,FLOW-IN)`=multi-line. Per spec, valid.

Spec-conformant (one-line enforced in flow-sequence pair):
```yaml
[ key: value ]                    # OK — one line
[
  key
  : value                         # REJECTED — flow-sequence pair uses hardcoded FLOW-KEY
]
```

Spec-conformant (one-line enforced when flow mapping is itself an implicit block key):
```yaml
{ a: b }: outer-value             # OK — fits on one line
{
  a: b
}: outer-value                    # REJECTED at outer scope — block-key must be one line
```

**Implementation evidence.** `event_iter/flow.rs` enforces single-line for flow sequences (`in_sequence` check); the in-code comment "Flow mappings `{...}` allow multi-line implicit keys — see YAML 1.2 §7.4.2" matches the BNF analysis above. The de-facto behavior of mature parsers (libyaml, PyYAML, snakeyaml) follows the BNF — multi-line implicit keys in top-level flow mappings are accepted.

### [111] nb-double-one-line

BNF: `nb-double-one-line ::= nb-double-char*`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "Double-quoted scalars are restricted to a single line when contained inside an implicit key."
- **Implementation:** Single-line fast path in `try_consume_double_quoted()` in `lexer/quoted.rs` — scanning stops at closing `"` without consuming a newline
- **Tests:** `tests/yaml-test-suite/src/LQZ7.yaml`
- **Rationale:** Hex-escape strictness is at [59]/[60]/[61]; [111] correctly composes them.

### [112] s-double-escaped(n)

BNF: `s-double-escaped(n) ::= s-white* c-escape b-non-content l-empty(n,FLOW-IN)* s-flow-line-prefix(n)`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "It is also possible to escape the line break character. In this case, the escaped line break is excluded from the content and any trailing white space characters that precede the escaped line break are preserved."
- **Implementation:** Escaped-newline handling in `try_consume_double_quoted()` in `lexer/quoted.rs` — `\` at end of line with optional trailing whitespace consumed; newline excluded from value; leading whitespace on next line preserved
- **Tests:** `tests/yaml-test-suite/src/NP9H.yaml` (Spec Example 7.5. Double Quoted Line Breaks)

### [113] s-double-break(n)

BNF: `s-double-break(n) ::= s-double-escaped(n) | s-flow-folded(n)`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "In a multi-line double-quoted scalar, line breaks are subject to flow line folding."
- **Implementation:** Both branches in `try_consume_double_quoted()` in `lexer/quoted.rs` — `\\\n` escape handled as `s-double-escaped`; plain newline handled as `s-flow-folded`
- **Tests:** `tests/yaml-test-suite/src/NP9H.yaml`; `tests/yaml-test-suite/src/7A4E.yaml`

### [114] nb-ns-double-in-line

BNF: `nb-ns-double-in-line ::= ( s-white* ns-double-char )*`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "All leading and trailing white space characters on each line are excluded from the content."
- **Implementation:** Inner-line scanning in `try_consume_double_quoted()` in `lexer/quoted.rs` — whitespace between non-whitespace characters preserved; trailing whitespace excluded when line ends
- **Tests:** `tests/yaml-test-suite/src/7A4E.yaml`

### [115] s-double-next-line(n)

BNF: `s-double-next-line(n) ::= s-double-break(n) ( ns-double-char nb-ns-double-in-line ( s-double-next-line(n) | s-white* ) )?`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "All leading and trailing white space characters on each line are excluded from the content. Each continuation line must therefore contain at least one non-space character."
- **Implementation:** Multi-line loop in `try_consume_double_quoted()` in `lexer/quoted.rs` — each new line after a break checked for non-space content; empty lines accumulated as folded newlines
- **Tests:** `tests/yaml-test-suite/src/7A4E.yaml`

### [116] nb-double-multi-line(n)

BNF: `nb-double-multi-line(n) ::= nb-ns-double-in-line ( s-double-next-line(n) | s-white* )`

- **Verdict:** Strict-conformant
- **Spec (§7.3.1):** "All leading and trailing white space characters on each line are excluded from the content."
- **Implementation:** Multi-line double-quoted path in `try_consume_double_quoted()` in `lexer/quoted.rs` — `nb-ns-double-in-line` on first line, then continuation via `s-double-break` loop
- **Tests:** `tests/yaml-test-suite/src/7A4E.yaml`

### [117] c-quoted-quote

BNF: `c-quoted-quote ::= "''"`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "Within a single-quoted scalar, such characters need to be repeated. This is the only form of escaping performed in single-quoted scalars."
- **Implementation:** `scan_single_quoted_line()` in `lexer/quoted.rs` — detects `''` as an escaped `'` and includes one `'` in the output
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/4GC6.yaml` (Spec Example 7.7. Single Quoted Characters)

### [118] nb-single-char

BNF: `nb-single-char ::= c-quoted-quote | ( nb-json - c-single-quote )`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "This restricts single-quoted scalars to printable characters."
- **Implementation:** `try_consume_single_quoted()` in `lexer/quoted.rs` — body scanning: all `nb-json` chars except `'` pass through; `''` decoded to `'`
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/4GC6.yaml`

### [119] ns-single-char

BNF: `ns-single-char ::= nb-single-char - s-white`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "In addition, it is only possible to break a long single-quoted line where a space character is surrounded by non-spaces."
- **Implementation:** Continuation-line scanning in `try_consume_single_quoted()` in `lexer/quoted.rs` — leading/trailing whitespace stripped; only non-whitespace characters initiate next-line content
- **Tests:** `tests/yaml-test-suite/src/PRH3.yaml` (Spec Example 7.9. Single Quoted Lines)

### [120] c-single-quoted(n,c)

BNF: `c-single-quoted(n,c) ::= c-single-quote nb-single-text(n,c) c-single-quote`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "The single-quoted style is specified by surrounding `'` indicators."
- **Implementation:** `try_consume_single_quoted()` in `lexer/quoted.rs` — opening `'` detected, body consumed, closing `'` required
- **Tests:** `rlsp-yaml-parser/tests/smoke/quoted_scalars.rs`; `tests/yaml-test-suite/src/87E4.yaml` (Spec Example 7.8. Single Quoted Implicit Keys)

### [121] nb-single-text(n,c)

BNF: `nb-single-text(FLOW-OUT) ::= nb-single-multi-line(n)` / `nb-single-text(FLOW-IN) ::= nb-single-multi-line(n)` / `nb-single-text(BLOCK-KEY) ::= nb-single-one-line` / `nb-single-text(FLOW-KEY) ::= nb-single-one-line`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "Single-quoted scalars are restricted to a single line when contained inside a implicit key."
- **Implementation:** Multi-line path taken in `try_consume_single_quoted()` in `lexer/quoted.rs` when closing `'` not found on first line; implicit-key context enforced by flow/block parsers
- **Tests:** `tests/yaml-test-suite/src/87E4.yaml` (single-line); `tests/yaml-test-suite/src/PRH3.yaml` (multi-line)
- **Rationale:** Same §7.4.2-vs-§7.3.x BNF analysis as [110]. The single-quoted text production has the same shape: FLOW-OUT/FLOW-IN → multi-line allowed; BLOCK-KEY/FLOW-KEY hardcoded sites → one-line.

### [122] nb-single-one-line

BNF: `nb-single-one-line ::= nb-single-char*`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "Single-quoted scalars are restricted to a single line when contained inside a implicit key."
- **Implementation:** Single-line fast path in `try_consume_single_quoted()` in `lexer/quoted.rs` — scanning stops at closing `'` without consuming a newline
- **Tests:** `tests/yaml-test-suite/src/87E4.yaml`

### [123] nb-ns-single-in-line

BNF: `nb-ns-single-in-line ::= ( s-white* ns-single-char )*`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "All leading and trailing white space characters are excluded from the content."
- **Implementation:** Inner-line whitespace between non-whitespace characters preserved in `try_consume_single_quoted()` in `lexer/quoted.rs`; trailing whitespace excluded
- **Tests:** `tests/yaml-test-suite/src/PRH3.yaml`

### [124] s-single-next-line(n)

BNF: `s-single-next-line(n) ::= s-flow-folded(n) ( ns-single-char nb-ns-single-in-line ( s-single-next-line(n) | s-white* ) )?`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character."
- **Implementation:** Multi-line loop in `try_consume_single_quoted()` in `lexer/quoted.rs` — each continuation line after `s-flow-folded` folding checked for non-space content
- **Tests:** `tests/yaml-test-suite/src/PRH3.yaml`

### [125] nb-single-multi-line(n)

BNF: `nb-single-multi-line(n) ::= nb-ns-single-in-line ( s-single-next-line(n) | s-white* )`

- **Verdict:** Strict-conformant
- **Spec (§7.3.2):** "All leading and trailing white space characters are excluded from the content."
- **Implementation:** Multi-line single-quoted path in `try_consume_single_quoted()` in `lexer/quoted.rs` — `nb-ns-single-in-line` on first line, then continuation via `s-flow-folded` loop
- **Tests:** `tests/yaml-test-suite/src/PRH3.yaml`

### [126] ns-plain-first(c)

BNF: `ns-plain-first(c) ::= ( ns-char - c-indicator ) | ( ( c-mapping-key | c-mapping-value | c-sequence-entry ) [ lookahead = ns-plain-safe(c) ] )`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Plain scalars must not begin with most indicators. However, the `:`, `?` and `-` indicators may be used as the first character if followed by a non-space 'safe' character."
- **Implementation:** `ns_plain_first_block()` in `lexer/plain.rs` — `is_c_indicator()` check; `?`, `:`, `-` allowed if followed by `ns_plain_safe_block()`; `scan_plain_line_flow()` in `lexer/plain.rs` for flow context
- **Tests:** `tests/yaml-test-suite/src/DBG4.yaml` (Spec Example 7.10. Plain Characters); `plain.rs` unit tests `scan_plain_line_block_cases`

### [127] ns-plain-safe(c)

BNF: `ns-plain-safe(FLOW-OUT) ::= ns-plain-safe-out` / `ns-plain-safe(FLOW-IN) ::= ns-plain-safe-in` / `ns-plain-safe(BLOCK-KEY) ::= ns-plain-safe-out` / `ns-plain-safe(FLOW-KEY) ::= ns-plain-safe-in`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Plain scalars must never contain the `: ` and ` #` character combinations. In addition, inside flow collections, plain scalars must not contain the `[`, `]`, `{`, `}` and `,` characters."
- **Implementation:** `ns_plain_safe_block()` in `lexer/plain.rs` — any `ns-char` for block/BLOCK-KEY context; `scan_plain_line_flow()` in `lexer/plain.rs` additionally stops at `,`, `[`, `]`, `{`, `}`
- **Tests:** `tests/yaml-test-suite/src/DBG4.yaml`

### [128] ns-plain-safe-out

BNF: `ns-plain-safe-out ::= ns-char`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Plain scalars must never contain the `: ` and ` #` character combinations."
- **Implementation:** `ns_plain_safe_block()` in `lexer/plain.rs` delegates to `is_ns_char()`
- **Tests:** `tests/yaml-test-suite/src/DBG4.yaml`
- **Rationale:** Audit A propagated leniency from [34] ns-char up to [128]. Per the symmetric reconciliation principle, leniency is attributed to [34] where the rule is enforced; [128] correctly composes it. Now that [34]'s enforcement gap is fixed (commit `666e2f2`), this remains Strict-conformant.

### [129] ns-plain-safe-in

BNF: `ns-plain-safe-in ::= ns-char - c-flow-indicator`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Inside flow collections, plain scalars must not contain the `[`, `]`, `{`, `}` and `,` characters."
- **Implementation:** `scan_plain_line_flow()` in `lexer/plain.rs` — terminates at `,`, `[`, `]`, `{`, `}` in addition to block-context terminators
- **Tests:** `tests/yaml-test-suite/src/DBG4.yaml`

### [130] ns-plain-char(c)

BNF: `ns-plain-char(c) ::= ( ns-plain-safe(c) - c-mapping-value - c-comment ) | ( [ lookbehind = ns-char ] c-comment ) | ( c-mapping-value [ lookahead = ns-plain-safe(c) ] )`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Plain scalars must never contain the `: ` and ` #` character combinations."
- **Implementation:** `ns_plain_char_block()` in `lexer/plain.rs` — `#` allowed only when NOT preceded by whitespace; `:` allowed only when followed by `ns_plain_safe_block()`; `scan_plain_line_flow()` uses same logic
- **Tests:** `tests/yaml-test-suite/src/DBG4.yaml`; `plain.rs` unit tests `scan_plain_line_block_cases`

### [131] ns-plain(n,c)

BNF: `ns-plain(n,FLOW-OUT) ::= ns-plain-multi-line(n,FLOW-OUT)` / `ns-plain(n,FLOW-IN) ::= ns-plain-multi-line(n,FLOW-IN)` / `ns-plain(n,BLOCK-KEY) ::= ns-plain-one-line(BLOCK-KEY)` / `ns-plain(n,FLOW-KEY) ::= ns-plain-one-line(FLOW-KEY)`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Plain scalars are further restricted to a single line when contained inside an implicit key."
- **Implementation:** `try_consume_plain_scalar()` in `lexer/plain.rs` for block multi-line; `scan_plain_line_flow()` for flow single-line in key/flow context; multi-line in FLOW-OUT/FLOW-IN via `collect_plain_continuations()`
- **Tests:** `tests/yaml-test-suite/src/L9U5.yaml` (Spec Example 7.11. Plain Implicit Keys); `tests/yaml-test-suite/src/HS5T.yaml` (Spec Example 7.12. Plain Lines)
- **Rationale:** Same §7.4.2-vs-§7.3.x BNF analysis as [110]. The plain scalar production has the same shape: FLOW-OUT/FLOW-IN → multi-line; BLOCK-KEY/FLOW-KEY → one-line.

### [132] nb-ns-plain-in-line(c)

BNF: `nb-ns-plain-in-line(c) ::= ( s-white* ns-plain-char(c) )*`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "In addition to a restricted character set, a plain scalar must not be empty or contain leading or trailing white space characters."
- **Implementation:** `scan_plain_line_block()` in `lexer/plain.rs` — inner loop: whitespace between tokens preserved; trailing whitespace excluded via `committed_end`; `scan_plain_line_flow()` uses same pattern
- **Tests:** `tests/yaml-test-suite/src/DBG4.yaml`

### [133] ns-plain-one-line(c)

BNF: `ns-plain-one-line(c) ::= ns-plain-first(c) nb-ns-plain-in-line(c)`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "Plain scalars are further restricted to a single line when contained inside an implicit key."
- **Implementation:** `peek_plain_scalar_first_line()` in `lexer/plain.rs` — first char checked via `ns_plain_first_block()`, remaining scanned via `scan_plain_line_block()`; same in `scan_plain_line_flow()` for flow-key context
- **Tests:** `tests/yaml-test-suite/src/L9U5.yaml`

### [134] s-ns-plain-next-line(n,c)

BNF: `s-ns-plain-next-line(n,c) ::= s-flow-folded(n) ns-plain-char(c) nb-ns-plain-in-line(c)`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "All leading and trailing white space characters are excluded from the content. Each continuation line must therefore contain at least one non-space character."
- **Implementation:** `collect_plain_continuations()` in `lexer/plain.rs` — blank lines accumulated as pending newlines; non-empty continuation line must have `scan_plain_line_block()` produce a non-empty result
- **Tests:** `tests/yaml-test-suite/src/HS5T.yaml`

### [135] ns-plain-multi-line(n,c)

BNF: `ns-plain-multi-line(n,c) ::= ns-plain-one-line(c) s-ns-plain-next-line(n,c)*`

- **Verdict:** Strict-conformant
- **Spec (§7.3.3):** "It is only possible to break a long plain line where a space character is surrounded by non-spaces."
- **Implementation:** `try_consume_plain_scalar()` in `lexer/plain.rs` — first line via `peek_plain_scalar_first_line()`, then zero or more continuation lines via `collect_plain_continuations()`
- **Tests:** `tests/yaml-test-suite/src/HS5T.yaml`

### [136] in-flow(n,c)

BNF: `in-flow(n,FLOW-OUT) ::= ns-s-flow-seq-entries(n,FLOW-IN)` / `in-flow(n,FLOW-IN) ::= ns-s-flow-seq-entries(n,FLOW-IN)` / `in-flow(n,BLOCK-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)` / `in-flow(n,FLOW-KEY) ::= ns-s-flow-seq-entries(n,FLOW-KEY)`

- **Verdict:** Not-applicable
- **Spec (§7.4):** "A flow collection may be nested within a block collection (FLOW-OUT context), nested within another flow collection (FLOW-IN context) or be a part of an implicit key."
- **Implementation:** (no implementation obligation)
- **Tests:** (no implementation obligation)
- **Rationale:** The BNF for `in-flow(n,c)` is purely a context-mapping function — it maps outer-context labels to the inner context for the entries production. It has no parsing semantics of its own; the parsing happens in the `ns-s-flow-seq-entries` production it forwards to. Consistent with §3 and §4's meta-entries.

### [137] c-flow-sequence(n,c)

BNF: `c-flow-sequence(n,c) ::= c-sequence-start s-separate(n,c)? in-flow(n,c)? c-sequence-end`

- **Verdict:** Strict-conformant
- **Spec (§7.4.1):** "Flow sequence content is denoted by surrounding `[` and `]` characters."
- **Implementation:** `[` branch pushes `FlowFrame::Sequence` in `event_iter/flow.rs`; `]` branch pops it
- **Tests:** `tests/yaml-test-suite/src/5KJE.yaml` (Spec Example 7.13. Flow Sequence)

### [138] ns-s-flow-seq-entries(n,c)

BNF: `ns-s-flow-seq-entries(n,c) ::= ns-flow-seq-entry(n,c) s-separate(n,c)? ( c-collect-entry s-separate(n,c)? ns-s-flow-seq-entries(n,c)? )?`

- **Verdict:** Strict-conformant
- **Spec (§7.4.1):** "Sequence entries are separated by a `,` character."
- **Implementation:** `,` branch inside `FlowFrame::Sequence` in `event_iter/flow.rs` — advances `has_value`, permits trailing comma before `]`
- **Tests:** `tests/yaml-test-suite/src/5KJE.yaml`; `tests/yaml-test-suite/src/8UDB.yaml`

### [139] ns-flow-seq-entry(n,c)

BNF: `ns-flow-seq-entry(n,c) ::= ns-flow-pair(n,c) | ns-flow-node(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.1):** "Any flow node may be used as a flow sequence entry. In addition, YAML provides a compact notation for a flow sequence entry that is a mapping with a single key/value pair."
- **Implementation:** Sequence item dispatch in `event_iter/flow.rs` — scalars, nested collections, and single-pair implicit mappings via `:` detection within `FlowFrame::Sequence`
- **Tests:** `tests/yaml-test-suite/src/8UDB.yaml`

### [140] c-flow-mapping(n,c)

BNF: `c-flow-mapping(n,c) ::= c-mapping-start s-separate(n,c)? ns-s-flow-map-entries(n,in-flow(c))? c-mapping-end`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "Flow mappings are denoted by surrounding `{` and `}` characters."
- **Implementation:** `{` branch pushes `FlowFrame::Mapping` in `event_iter/flow.rs`; `}` branch pops it
- **Tests:** `tests/yaml-test-suite/src/5C5M.yaml` (Spec Example 7.15. Flow Mappings)

### [141] ns-s-flow-map-entries(n,c)

BNF: `ns-s-flow-map-entries(n,c) ::= ns-flow-map-entry(n,c) s-separate(n,c)? ( c-collect-entry s-separate(n,c)? ns-s-flow-map-entries(n,c)? )?`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "Mapping entries are separated by a `,` character."
- **Implementation:** `,` branch inside `FlowFrame::Mapping` in `event_iter/flow.rs` — resets to Key phase, permits trailing comma before `}`
- **Tests:** `tests/yaml-test-suite/src/5C5M.yaml`; `tests/yaml-test-suite/src/DFF7.yaml`

### [142] ns-flow-map-entry(n,c)

BNF: `ns-flow-map-entry(n,c) ::= ( c-mapping-key s-separate(n,c) ns-flow-map-explicit-entry(n,c) ) | ns-flow-map-implicit-entry(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "If the optional `?` mapping key indicator is specified, the rest of the entry may be completely empty."
- **Implementation:** `?` explicit-key indicator branch inside `FlowFrame::Mapping` in `event_iter/flow.rs`; implicit-key entry falls through to scalar/collection dispatch
- **Tests:** `tests/yaml-test-suite/src/DFF7.yaml`

### [143] ns-flow-map-explicit-entry(n,c)

BNF: `ns-flow-map-explicit-entry(n,c) ::= ns-flow-map-implicit-entry(n,c) | ( e-node e-node )`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "If the optional `?` mapping key indicator is specified, the rest of the entry may be completely empty."
- **Implementation:** When `}` arrives with `explicit_key_pending = true` in Key phase in `event_iter/flow.rs`, two `empty_scalar_event()` pushed — empty key and empty value
- **Tests:** `tests/yaml-test-suite/src/DFF7.yaml`

### [144] ns-flow-map-implicit-entry(n,c)

BNF: `ns-flow-map-implicit-entry(n,c) ::= ns-flow-map-yaml-key-entry(n,c) | c-ns-flow-map-empty-key-entry(n,c) | c-ns-flow-map-json-key-entry(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "Normally, YAML insists the `:` mapping value indicator be separated from the value by white space."
- **Implementation:** Dispatch on current char in `event_iter/flow.rs` — `:` alone → empty key entry; quoted scalar → JSON-key entry; plain scalar or nested collection → YAML-key entry
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml`

### [145] ns-flow-map-yaml-key-entry(n,c)

BNF: `ns-flow-map-yaml-key-entry(n,c) ::= ns-flow-yaml-node(n,c) ( ( s-separate(n,c)? c-ns-flow-map-separate-value(n,c) ) | e-node )`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "The `:` character can be used inside plain scalars, as long as it is not followed by white space."
- **Implementation:** Plain scalar or nested collection as key in `FlowFrame::Mapping` Key phase in `event_iter/flow.rs`; `:` with trailing space or flow indicator consumed in Value phase
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml`

### [146] c-ns-flow-map-empty-key-entry(n,c)

BNF: `c-ns-flow-map-empty-key-entry(n,c) ::= e-node c-ns-flow-map-separate-value(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "Note that the value may be completely empty since its existence is indicated by the `:`."
- **Implementation:** `:` at start of key position in `FlowFrame::Mapping` in `event_iter/flow.rs` → `empty_scalar_event()` pushed for empty key, then value consumed
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml`

### [147] c-ns-flow-map-separate-value(n,c)

BNF: `c-ns-flow-map-separate-value(n,c) ::= c-mapping-value [ lookahead ≠ ns-plain-safe(c) ] ( ( s-separate(n,c) ns-flow-node(n,c) ) | e-node )`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "The `:` character can be used inside plain scalars, as long as it is not followed by white space. This allows for unquoted URLs and timestamps."
- **Implementation:** `:` in Value phase in `event_iter/flow.rs` checked for trailing space/flow-indicator via `ns_plain_safe_block()` lookahead; `:x` treated as plain-scalar content not a separator
- **Tests:** `tests/yaml-test-suite/src/4ABK.yaml`; `tests/yaml-test-suite/src/DFF7.yaml`

### [148] c-ns-flow-map-json-key-entry(n,c)

BNF: `c-ns-flow-map-json-key-entry(n,c) ::= c-flow-json-node(n,c) ( ( s-separate(n,c)? c-ns-flow-map-adjacent-value(n,c) ) | e-node )`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "To ensure JSON compatibility, if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the `:`."
- **Implementation:** Quoted scalar as key in `FlowFrame::Mapping` Key phase in `event_iter/flow.rs`; `:` without mandatory preceding space allowed immediately after closing `"` or `'`
- **Tests:** `tests/yaml-test-suite/src/C2DT.yaml` (Spec Example 7.18. Flow Mapping Adjacent Values)

### [149] c-ns-flow-map-adjacent-value(n,c)

BNF: `c-ns-flow-map-adjacent-value(n,c) ::= c-mapping-value ( ( s-separate(n,c)? ns-flow-node(n,c) ) | e-node )`

- **Verdict:** Strict-conformant
- **Spec (§7.4.2):** "To ensure JSON compatibility, if a key inside a flow mapping is JSON-like, YAML allows the following value to be specified adjacent to the `:`."
- **Implementation:** `:` in Value phase after a JSON-like key in `event_iter/flow.rs` — space before value is optional; value may be omitted → `empty_scalar_event()`
- **Tests:** `tests/yaml-test-suite/src/C2DT.yaml`

### [150] ns-flow-pair(n,c)

BNF: `ns-flow-pair(n,c) ::= ( c-mapping-key s-separate(n,c) ns-flow-map-explicit-entry(n,c) ) | ns-flow-pair-entry(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.3):** "A more compact notation is usable inside flow sequences, if the mapping contains a single key/value pair."
- **Implementation:** `:` value separator inside `FlowFrame::Sequence` in `event_iter/flow.rs` triggers single-pair implicit mapping — `MappingStart` inserted before the key, `MappingEnd` emitted before next `,` or `]`
- **Tests:** `tests/yaml-test-suite/src/QF4Y.yaml` (Spec Example 7.19. Single Pair Flow Mappings); `tests/yaml-test-suite/src/CT4Q.yaml`

### [151] ns-flow-pair-entry(n,c)

BNF: `ns-flow-pair-entry(n,c) ::= ns-flow-pair-yaml-key-entry(n,c) | c-ns-flow-map-empty-key-entry(n,c) | c-ns-flow-pair-json-key-entry(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.3):** "A more compact notation is usable inside flow sequences."
- **Implementation:** Dispatch within `FlowFrame::Sequence` in `event_iter/flow.rs` after `:` detected — plain/alias key, empty-key, or quoted-JSON key
- **Tests:** `tests/yaml-test-suite/src/9MMW.yaml`

### [152] ns-flow-pair-yaml-key-entry(n,c)

BNF: `ns-flow-pair-yaml-key-entry(n,c) ::= ns-s-implicit-yaml-key(FLOW-KEY) c-ns-flow-map-separate-value(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.3):** "A more compact notation is usable inside flow sequences."
- **Implementation:** Plain scalar key in sequence entry followed by `:` separator in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/9MMW.yaml`

### [153] c-ns-flow-pair-json-key-entry(n,c)

BNF: `c-ns-flow-pair-json-key-entry(n,c) ::= c-s-implicit-json-key(FLOW-KEY) c-ns-flow-map-adjacent-value(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.4.3):** "A more compact notation is usable inside flow sequences."
- **Implementation:** Quoted scalar as key in sequence entry, adjacent `:` separator in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/9MMW.yaml`

### [154] ns-s-implicit-yaml-key(c)

BNF: `ns-s-implicit-yaml-key(c) ::= ns-flow-yaml-node(0,c) s-separate-in-line? /* At most 1024 characters altogether */`

- **Verdict:** Strict-conformant
- **Spec (§7.4.3):** "To limit the amount of lookahead required, the `:` indicator must appear at most 1024 Unicode characters beyond the start of the key. In addition, the key is restricted to a single line."
- **Implementation:** Single-line restriction and 1024-Unicode-character limit both enforced in `event_iter/flow.rs`; plain YAML-key and quoted JSON-key forms share the same check via `key_start_byte` tracking
- **Tests:** `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [155] c-s-implicit-json-key(c)

BNF: `c-s-implicit-json-key(c) ::= c-flow-json-node(0,c) s-separate-in-line? /* At most 1024 characters altogether */`

- **Verdict:** Strict-conformant
- **Spec (§7.4.3):** "To limit the amount of lookahead required, the `:` indicator must appear at most 1024 Unicode characters beyond the start of the key."
- **Implementation:** Quoted JSON-key start byte recorded at flow `:` detection in `event_iter/flow.rs`; shared 1024-char check covers both plain and quoted implicit keys
- **Tests:** `rlsp-yaml-parser/tests/implicit_key_length.rs` (groups A–N and H5–H8, 48 cases)

### [156] ns-flow-yaml-content(n,c)

BNF: `ns-flow-yaml-content(n,c) ::= ns-plain(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.5):** "The only flow style that does not have explicit start and end indicators is the plain scalar."
- **Implementation:** `scan_plain_line_flow()` in `lexer/plain.rs` for flow context; `try_consume_plain_scalar()` in `lexer/plain.rs` for block context plain scalars
- **Tests:** `tests/yaml-test-suite/src/Q88A.yaml` (Spec Example 7.23. Flow Content — plain case)

### [157] c-flow-json-content(n,c)

BNF: `c-flow-json-content(n,c) ::= c-flow-sequence(n,c) | c-flow-mapping(n,c) | c-single-quoted(n,c) | c-double-quoted(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.5):** "JSON-like flow styles all have explicit start and end indicators."
- **Implementation:** All four — `[`, `{`, `'`, `"` — dispatch to their respective handlers in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/Q88A.yaml`

### [158] ns-flow-content(n,c)

BNF: `ns-flow-content(n,c) ::= ns-flow-yaml-content(n,c) | c-flow-json-content(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.5):** "JSON-like flow styles all have explicit start and end indicators. The only flow style that does not have this property is the plain scalar."
- **Implementation:** Unified dispatch in the main character-dispatch loop in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/Q88A.yaml`

### [159] ns-flow-yaml-node(n,c)

BNF: `ns-flow-yaml-node(n,c) ::= c-ns-alias-node | ns-flow-yaml-content(n,c) | ( c-ns-properties(n,c) ( ( s-separate(n,c) ns-flow-yaml-content(n,c) ) | e-scalar ) )`

- **Verdict:** Strict-conformant
- **Spec (§7.5):** "A complete flow node also has optional node properties, except for alias nodes."
- **Implementation:** Alias at `*`, anchor/tag properties at `&`/`!`, then plain scalar or nested collection in `event_iter/flow.rs`; empty scalar when properties present but no content follows
- **Tests:** `tests/yaml-test-suite/src/LE5A.yaml` (Spec Example 7.24. Flow Nodes)

### [160] c-flow-json-node(n,c)

BNF: `c-flow-json-node(n,c) ::= ( c-ns-properties(n,c) s-separate(n,c) )? c-flow-json-content(n,c)`

- **Verdict:** Strict-conformant
- **Spec (§7.5):** "A complete flow node also has optional node properties."
- **Implementation:** Tag/anchor properties scanned before `"`, `'`, `[`, `{` dispatch in `event_iter/flow.rs`
- **Tests:** `tests/yaml-test-suite/src/LE5A.yaml` — `!!str "a"`, `&anchor "c"` cases

### [161] ns-flow-node(n,c)

BNF: `ns-flow-node(n,c) ::= c-ns-alias-node | ns-flow-content(n,c) | ( c-ns-properties(n,c) ( ( s-separate(n,c) ns-flow-content(n,c) ) | e-scalar ) )`

- **Verdict:** Strict-conformant
- **Spec (§7.5):** "A complete flow node also has optional node properties."
- **Implementation:** Top-level dispatch in the flow parser loop in `event_iter/flow.rs` — alias, properties + content, or bare content; empty scalar when properties present but content absent
- **Tests:** `tests/yaml-test-suite/src/LE5A.yaml`
