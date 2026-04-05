# rlsp-yaml-parser Conformance Gap Analysis

**Date:** 2026-04-05
**Crate:** `rlsp-yaml-parser`
**Test suite:** YAML Test Suite (commit `da267a5c`)
**Total test cases:** ~402 (308 valid + 94 invalid)
**Failures:** 114 (58 valid rejected + 56 invalid accepted)
**Pass rate:** 71.6% overall (81.2% valid, 40.4% invalid detection)

---

## Executive Summary

The initial `rlsp-yaml-parser` build (plan `2026-04-04-rlsp-yaml-parser`)
delivered all 12 tasks including a conformance test harness, but stopped
at 114 test failures — well short of the 100%/100% target. The failures
split into two categories: 58 cases where valid YAML is incorrectly
rejected ("unexpected parse error"), and 56 cases where invalid YAML is
incorrectly accepted ("expected parse error, got clean parse").

The valid-YAML failures indicate production bugs in the parser — these
affect real-world YAML files. The invalid-YAML acceptance indicates
missing validation — the parser is too permissive. Both must be resolved
before the parser can replace saphyr in `rlsp-yaml`.

For comparison, saphyr scores 89.6% valid / 70.2% invalid on the same
test matrix. Our current 81.2% valid rate is **worse than saphyr** on
valid input, which defeats the purpose of the replacement. The invalid
detection rate (40.4%) is also below saphyr's 70.2%.

---

## Failure Breakdown by Type

### Valid YAML incorrectly rejected (58 failures)

These are parser bugs — the parser fails to parse YAML that the spec
defines as valid.

#### Block Scalar and Line Folding (9 failures)

The block scalar productions (§8) and line folding rules (§6) have
several bugs. These affect literal (`|`) and folded (`>`) block scalars,
empty line handling, and chomping behavior.

| Test | Description | Error Location | Root Cause Hypothesis |
|------|-------------|----------------|----------------------|
| 5GBF | Spec Example 6.5. Empty Lines | line 2 | Tab in empty line within double-quoted scalar |
| 6WPF | Spec Example 6.8. Flow Folding | line 2 | Multiline double-quoted with indented continuation |
| 7T8X | Spec 8.10. Folded Lines | line 8 | Folded scalar with more-indented lines (bullet list) |
| F6MC | More indented lines at beginning of folded | line 3 | `>2` explicit indent with more-indented first line |
| F8F9 | Spec 8.5. Chomping Trailing Lines | — | Chomping indicator interaction with trailing newlines |
| H2RW | Blank lines | — | Blank line handling in block context |
| M29M | Literal Block Scalar | — | Literal scalar content parsing |
| MYW6 | Block Scalar Strip | — | Strip chomping (`|-`) behavior |
| NB6Z | Multiline plain with tabs on empty lines | — | Tab on otherwise-empty continuation line |

**Common thread:** Most involve empty lines, tabs on empty lines, or
more-indented lines within block scalars. The `l_empty` [70],
`b_l_trimmed` [71], and `b_l_folded` [73] productions likely need fixes.

#### Block Structure (6 failures)

Block sequence and mapping handling fails on several spec examples.

| Test | Description | YAML Pattern |
|------|-------------|-------------|
| 93JH | Block Mappings in Block Sequence | Indented sequence with mapping entries |
| 9U5K | Spec 2.12. Compact Nested Mapping | `- item: value` compact sequence-of-mappings |
| JQ4R | Spec 8.14. Block Sequence | Basic block sequence |
| S3PD | Spec 8.18. Implicit Block Mapping | Implicit entries |
| V9D5 | Spec 8.19. Compact Block Mappings | `? key\n: value` compact form |
| 735Y | Spec 8.20. Block Node Types | Mixed flow-in-block and block scalar |

**Common thread:** Compact notation and indentation detection for
`s_b_block_indented` [185] and `ns_l_compact_sequence` [186] /
`ns_l_compact_mapping` [195].

#### Flow Multiline (7 failures)

Multiline keys and values within flow collections fail consistently.

| Test | Description | YAML Pattern |
|------|-------------|-------------|
| 8KB6 | Multiline plain flow mapping key without value | `{ multi\n  line, a: b}` |
| 9BXH | Multiline double-quoted flow key without value | `{ "multi\n  line", a: b}` |
| 9SA2 | Multiline double-quoted flow key | `{ "multi\n  line": value}` |
| M7NX | Nested flow collections | Deep nesting with line breaks |
| NJ66 | Multiline plain flow mapping key | Plain key spanning lines |
| VJP3 | Flow collections over many lines | Long flow split across lines |
| LP6E | Whitespace after scalars in flow | Trailing spaces in flow entries |

**Common thread:** Flow content continuation across lines. The
`c_s_implicit_json_key` [161] and multiline flow entry productions
likely truncate too early or fail to handle line continuation.

#### Anchors and Node Properties (8 failures)

Anchor placement in various positions fails.

| Test | Description | YAML Pattern |
|------|-------------|-------------|
| 6BFJ | Mapping, key and flow sequence item anchors | `&mapping\n&key [ &item a ]` |
| 6M2F | Aliases in Explicit Block Mapping | `? &a a\n: &b b\n: *a` |
| 7BMT | Node and Mapping Key Anchors | Multiple anchors on keys/values |
| E76Z | Aliases in Implicit Block Mapping | `&a a: &b b\n*b : *a` |
| U3XV | Node and Mapping Key Anchors | Anchors on implicit keys |
| 9WXW | Spec 6.18. Primary Tag Handle | `!foo "bar"` with tag directive |
| HMQ5 | Spec 6.23. Node Properties | Combined anchor + tag |
| P76L | Spec 6.19. Secondary Tag Handle | `!!int` secondary handle |

**Common thread:** `c_ns_properties` [96] interaction with implicit
keys and the key/value separator. The properties production may consume
too little or fail to compose with `ns_s_block_map_implicit_key` [193].

#### Plain and Quoted Scalars (8 failures)

| Test | Description | YAML Pattern |
|------|-------------|-------------|
| 5MUD | Colon and adjacent value on next line | `{ "foo"\n  :bar }` |
| DBG4 | Spec 7.10. Plain Characters | `::vector`, `: - ()`, `-123` |
| FBC9 | Allowed characters in plain scalars | Special chars in plain context |
| K3WX | Colon and adjacent value after comment | Colon on next line after comment |
| S7BG | Colon followed by comma | Flow mapping with `:,` pattern |
| NAT4 | Various empty/newline quoted strings | Empty and newline-only quotes |
| Q8AD | Spec 7.5. Double Quoted Line Breaks | `"folded\nline"` multiline |
| T4YY | Spec 7.9. Single Quoted Lines | Multiline single-quoted |

**Common thread:** `ns_plain_char` [130] and `ns_plain_first` [126]
may be too restrictive, and quoted scalar multiline handling
(`s_double_next_line` [112], `s_single_next_line`) needs fixes.

#### Document Handling (5 failures)

| Test | Description | YAML Pattern |
|------|-------------|-------------|
| 82AN | Three dashes and content without space | `---word1` (bare document) |
| M7A3 | Spec 9.3. Bare Documents | Bare document edge cases |
| QT73 | Comment and document-end marker | `...` with comment |
| S4T7 | Document with footer | Document followed by `...` |
| UT92 | Spec 9.4. Explicit Documents | Multiple `---` documents |

**Common thread:** `c_forbidden` [205] detection and
`l_bare_document` [206] / `l_explicit_document` [207] interaction.

#### Tabs (4 failures)

| Test | Description |
|------|-------------|
| DC7X | Various trailing tabs — tabs after values, after sequence markers |
| DK95 [4,5] | Tab-only and space+tab "blank" lines between mappings |
| Y79Y [1] | Tab in various valid contexts |

**Common thread:** `s_white` [33] includes tabs, but several productions
may only accept spaces where tabs should also be valid.

#### Miscellaneous Spec Examples (11 failures)

| Test | Description |
|------|-------------|
| AZW3 | Lookahead — `"` and `]` in plain scalars |
| M5DY | Spec 2.11. Mapping between Sequences |
| RZP5 | Various Trailing Comments |
| RZT7 | Spec 2.28. Log File |
| S9E8 | Spec 5.3. Block Structure Indicators |
| UGM3 | Spec 2.27. Invoice |
| XW4D | Various Trailing Comments |
| ZF4X | Spec 2.6. Mapping of Mappings |
| 4FJ6 | Nested implicit complex keys |
| M2N8 | Question mark edge cases |
| NKF9 | Empty keys in block and flow mapping |

---

### Invalid YAML incorrectly accepted (56 failures)

These are missing validations — the parser accepts input that the YAML
spec defines as invalid.

#### Bad Indentation (7 failures)

Parser doesn't detect indentation violations:
- `4HVU` — sequence item with wrong indentation
- `9C9N` — flow sequence continuation not indented enough
- `DMG6` — mapping value with wrong indentation
- `N4JP`, `U44R` — bad indentation in mapping
- `QB6E` — wrong indented multiline quoted scalar
- `ZVH3` — wrong indented sequence item

#### Invalid Structure (9 failures)

Parser doesn't detect structural violations:
- `236B` — value after closed mapping
- `2CMS` — mapping syntax in plain multiline
- `5U3A` — sequence on same line as mapping key
- `6S55` — scalar after sequence
- `9CWY` — scalar at end of mapping
- `BD7L` — mapping after sequence at same level
- `TD5N` — scalar after sequence
- `ZCZ6` — `a: b: c: d` nested mapping in plain value
- `ZL4Z` — `'b': c` after `a:` on same line

#### Invalid Flow (7 failures)

- `9MAG` — leading comma in flow sequence `[, a, b]`
- `C2SP` — flow mapping key on two lines `[23\n]: 42`
- `CTN5` — extra comma in flow sequence `[a, b, c, ,]`
- `KS4U` — item after end of flow sequence
- `N782` — document markers inside flow
- `YJV2` — dash in flow sequence `[-]`
- `ZXT5` — implicit key followed by newline in flow

#### Comment Boundary Violations (4 failures)

- `8XDJ` — comment between continuation lines of plain scalar
- `BF9H` — trailing comment breaks plain scalar continuation
- `BS4K` — `word1  # comment\nword2` should not be a single scalar
- `GDY7` — comment that looks like a mapping key

#### Anchor Misuse (6 failures)

- `4JVG` — scalar value with two anchors
- `CXX2` — mapping with anchor on document start line
- `G9HC` — anchor in zero-indented sequence
- `GT5M` — node anchor in sequence (wrong position)
- `H7J7` — node anchor not indented
- `SY6V` — anchor before sequence entry on same line

#### Directive/Document Violations (12 failures)

- `5TRB` — document-start marker inside double-quoted string
- `9HCY` — missing document footer before directives
- `9MMA` — directive by itself with no document
- `9MQT` — `...` in double-quoted content not terminating
- `B63P` — directive without document
- `EB22` — missing document-end before directive
- `H7TQ` — extra words on `%YAML` directive
- `MUS6` — directive variants (2 sub-cases)
- `RHX7` — YAML directive without document-end marker
- `RXY3` — document-end marker in single-quoted string
- `SF5V` — duplicate YAML directive
- `QLJ7` — tag shorthand used but only defined in first doc

#### Other (11 failures)

- `7MNF` — missing colon (plain scalar vs mapping detection)
- `DK95` — tab inside double-quoted continuation
- `JKF3` — multiline unindented double-quoted block key
- `U99R` — invalid comma in tag
- `W9L4` — literal block scalar with more spaces in first line
- `Y79Y` — 5 sub-cases of tabs in invalid contexts

---

## Root Cause Analysis

The failures cluster around a small number of parser deficiencies:

### 1. Multiline continuation handling

The parser's line continuation logic (for quoted scalars, plain scalars,
and flow collections) is too simplistic. It fails when:
- Content continues on the next line with different indentation
- Tabs appear in continuation positions
- Comments interrupt continuation

**Affected productions:** `s_double_next_line` [112],
`s_single_next_line` [120], `s_ns_plain_next_line` [133],
`s_flow_folded` [74], `l_empty` [70]

### 2. Block scalar indentation and chomping

The block scalar implementation handles basic cases but fails on:
- More-indented lines (literal meaning in folded scalars)
- Auto-detected indentation with edge cases
- Chomping interaction with trailing empty lines
- Explicit indentation indicators (`>2`, `|4`)

**Affected productions:** `c_b_block_header` [162],
`l_literal_content` [174], `l_folded_content` [182],
`b_chomped_last` [165], `l_chomped_empty` [166]

### 3. Node properties in implicit keys

Anchors and tags before implicit mapping keys fail because
`c_ns_properties` [96] doesn't compose correctly with the
implicit key detection logic.

### 4. Missing validation checks

The parser is fundamentally a recognizer — it succeeds if it can
match the input against the grammar. But YAML also requires rejecting
certain patterns (bad indentation, structural violations). These
require explicit error checks that were not implemented.

### 5. Tab handling

Several productions only match spaces where the spec allows `s_white`
(which includes tabs). Trailing tabs, tabs on empty lines, and tabs
in separation contexts all fail.

---

## Comparison with Other Parsers

| Parser | Valid Pass | Invalid Detect | Our Status |
|--------|-----------|----------------|------------|
| libfyaml (C) | 100% | 100% | Target |
| HsYAML (Haskell) | 97.1% | 100% | Reference |
| saphyr (Rust) | 89.6% | 70.2% | **We're below this** |
| **rlsp-yaml-parser** | **81.2%** | **40.4%** | Current |

We are currently worse than saphyr on both metrics. The conformance
hardening plan must bring us to at least parity with HsYAML (97%+)
and ideally to 100%/100%.

---

## Remediation Plan

See `.ai/plans/2026-04-05-conformance-hardening.md` — 8 tasks
organized by failure category, valid-YAML fixes first (Tasks 1-6),
then invalid-YAML rejection (Task 7), final verification (Task 8).

**Estimated scope:** ~114 test fixes across 6 parser source files.
Many failures share root causes, so the actual number of production
fixes will be significantly smaller than 114.
