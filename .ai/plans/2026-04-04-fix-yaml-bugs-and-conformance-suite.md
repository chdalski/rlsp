**Repository:** root
**Status:** NotStarted
**Created:** 2026-04-04

## Goal

Fix 6 bugs in the YAML language server (formatter and validator) discovered
by testing against real-world Kubernetes/GitHub Actions files, then add a
conformance test suite using yaml-test-suite and real-world fixtures to
systematically catch more issues.

## Context

The user installed the VS Code extension and formatted `.github/workflows/ci.yml`.
Within a minute they spotted 6 bugs:

1. **Blank line deletion** — formatter strips blank lines between top-level
   mapping entries (`on:`, `permissions:`, `env:`, `jobs:`)
2. **`on:` becomes `"on":`** — formatter quotes YAML 1.1 reserved words used
   as plain scalar keys
3. **False `duplicateKey` errors** — validator reports `cpu`/`memory` as
   duplicates when they appear under sibling mappings (`max:`, `min:`,
   `default:`, `defaultRequest:`) at the same indent
4. **`flowMap` warning on `status: {}`** — validator warns on empty flow
   collections which are idiomatic YAML
5. **Flow-to-block sequence indentation** — converting
   `command: ["python", "-m", ...]` to block style loses indentation context
6. **Unnecessary quotes in block sequences** — `"python"` stays quoted after
   flow-to-block conversion when plain `python` is valid

The user wants a systematic approach: fix the known bugs, then add a
conformance test suite to discover unknown bugs.

### Root cause analysis

**Bug 1 (blank lines):** The formatter builds Doc IR from the parsed AST
(which has no blank-line concept) and renders it. `attach_comments` preserves
blank lines between comment groups (`pending_blanks`, line 119) but not
between bare content lines. Blank lines between content entries are silently
dropped.

**Bug 2 (`on:` quoting):** saphyr with `early_parse=true` (default, used by
`YamlOwned::load_from_str`) resolves all scalars to `Value` variants, losing
original scalar style. `on` becomes `Value(String("on"))`, then
`string_to_doc("on")` calls `needs_quoting("on")` which returns true (line
384), so the formatter adds quotes. The `Representation` variant handling in
`node_to_doc` (lines 296-308) correctly preserves original style but is
unreachable via the current parse path (documented at line 1438).

**Bug 3 (duplicate keys):** `validate_duplicate_keys` uses indent-based scope
tracking. Line 638: `scope_stack.retain(|(si, _)| *si <= effective_indent)`
keeps scopes at the same indent, so when `min:` (indent 6) follows `max:`
(indent 6), the stale scope from `max:` is retained. Lines 658-663 skip
creating a new scope because one already exists at that indent. Result: keys
from sibling mappings share a scope. No test exists for sibling mappings
under a common parent.

**Bug 4 (empty flow warnings):** `validate_flow_style` (line 190) warns on
ALL flow mappings/sequences including empty ones. But the formatter itself
emits `{}` and `[]` for empty collections (lines 490, 536), creating a
contradiction.

**Bug 5 (indentation):** The Doc IR structure for nested sequence-in-mapping-
in-sequence-item looks theoretically correct (`indent(concat([hard_line(),
sequence_to_doc(...)]))` at line 517), but the actual output disagrees. The
developer must write a reproducing test and trace the printer behavior to
identify the exact fault — the interaction between multiple nested `indent()`
calls and the printer's work-stack processing may have a subtle ordering bug.

**Bug 6 (unnecessary quotes):** Connected to Bug 2. With `early_parse=true`,
`"python"` in a flow sequence becomes `Value(String("python"))`, and
`string_to_doc` correctly outputs plain `python` (since `needs_quoting`
returns false). However, if we switch to `early_parse=false` (to fix Bug 2),
`"python"` becomes `Representation("python", DoubleQuoted, _)` and the
formatter would preserve the double quotes. The fix: for quoted
`Representation` variants, check `needs_quoting` — if the value doesn't need
quoting, emit as plain. This strips syntactic quotes (flow context) while
preserving semantic quotes (values that genuinely need quoting like `"5000"`
or `"true"`).

### Key files

- `rlsp-yaml/src/formatter.rs` — formatter (bugs 1, 2, 5, 6)
- `rlsp-yaml/src/validators.rs` — validators (bugs 3, 4)
- `rlsp-yaml/src/parser.rs` — parser entry point (bug 2 context)
- `rlsp-fmt/src/printer.rs` — pretty-printer (bug 5 context)
- `rlsp-fmt/src/ir.rs` — Doc IR types

## Steps

- [x] Clarify requirements with user
- [x] Update CLAUDE.md references (8f943fc)
- [ ] Fix duplicate key false positives (validator)
- [ ] Fix empty flow collection warnings (validator)
- [ ] Switch formatter to `early_parse(false)` for scalar style preservation
- [ ] Fix blank line preservation (formatter)
- [ ] Fix flow-to-block sequence indentation (formatter)
- [ ] Strip unnecessary quotes from Representation variants (formatter)
- [ ] Add conformance test suite infrastructure
- [ ] Add real-world ecosystem fixtures

## Tasks

### Task 1: Update CLAUDE.md references

Add YAML specification and ecosystem references to the project's `CLAUDE.md`
so all agents have context for spec-correct behavior.

Add to the References section:
- YAML 1.2 spec: https://yaml.org/spec/1.2.2/
- YAML test suite: https://github.com/yaml/yaml-test-suite
- YAML test matrix: https://matrix.yaml.info/
- Kubernetes API reference: https://kubernetes.io/docs/reference/
- KubeSpec: https://kubespec.dev/

- [x] Add references to `/workspace/CLAUDE.md`

### Task 2: Fix duplicate key false positives

Fix `validate_duplicate_keys` in `validators.rs` to correctly handle sibling
mappings at the same indent level.

The bug is in scope management: line 638 uses `*si <= effective_indent`
(keeps scopes at same indent), should use `*si < effective_indent` (pops
scopes at same indent, forcing a fresh scope for sibling mappings). This
ensures that when `min:` follows `max:` at the same indent, the stale keys
from `max:`'s children are cleared.

- [ ] Fix `scope_stack.retain` condition on line 638
- [ ] Add test: sibling mappings under common parent (K8s LimitRange pattern)
- [ ] Add test: deeply nested sibling mappings
- [ ] Verify existing tests still pass

### Task 3: Fix empty flow collection warnings

Suppress `flowMap`/`flowSeq` warnings for empty flow collections (`{}` and
`[]`).

In `validate_flow_style` (line 190), after finding a matching closing
brace/bracket, check if the content between open and close is empty
(whitespace-only). If so, skip emitting the diagnostic.

- [ ] Add empty-content check in flowMap detection (around line 207)
- [ ] Add empty-content check in flowSeq detection (around line 224)
- [ ] Add test: `status: {}` produces no warning
- [ ] Add test: `items: []` produces no warning
- [ ] Add test: `{a: 1}` still produces warning
- [ ] Verify existing tests still pass

### Task 4: Switch formatter to early_parse(false)

Change the formatter's YAML parsing to use saphyr's `early_parse(false)` so
scalar style information is preserved as `Representation` variants instead of
being resolved to `Value` variants. This is the foundation for fixing the
`on:` quoting bug and enabling quote stripping.

In `format_yaml` (line 244), replace `YamlOwned::load_from_str(text_input)`
with the `YamlLoader`/`YamlOwnedLoader` API using `early_parse(false)`. The
developer should investigate saphyr's API — there may be a `LoadSettings` or
builder pattern on the loader. Check the saphyr source at
`~/.cargo/registry/src/*/saphyr-0.0.6/` for the available API.

The `Representation` handling in `node_to_doc` (lines 296-308) already
correctly handles all scalar styles. The `Value` arms (lines 293-294,
327-335) become dead code — they should be kept as fallbacks but the primary
path shifts to `Representation`.

**Critical:** only change the parse call in `formatter.rs`. The `parser.rs`
parse function must continue using `early_parse=true` so the rest of the
language server (schema validation, completions, hover) gets resolved values.

- [ ] Find and use saphyr API for `YamlOwned` with `early_parse(false)`
- [ ] Update `format_yaml` to use the new parse call
- [ ] Add test: `on:` key stays unquoted after formatting
- [ ] Add test: `"on":` key stays quoted after formatting
- [ ] Add test: `true`/`false`/`null` plain scalars preserved
- [ ] Add test: numeric values preserved as-is
- [ ] Verify all existing formatter tests still pass

### Task 5: Fix blank line preservation

Preserve blank lines between content entries in the formatted output. Users
add blank lines to visually group related sections (e.g., separating `on:`,
`permissions:`, `env:`, `jobs:` in GitHub Actions files).

In `attach_comments` (line 108), the `pending_blanks` counter already tracks
blank lines between entries. Currently blank lines are only emitted between
comment groups (line 126-128). Extend this: when a content entry has
`pending_blanks > 0` and there are no pending leading comments, insert blank
lines before the entry's output.

- [ ] Track blank lines between content entries (not just comment groups)
- [ ] Insert preserved blank lines in `attach_comments` output
- [ ] Add test: blank lines between top-level keys preserved
- [ ] Add test: blank lines inside nested mappings preserved
- [ ] Add test: multiple consecutive blank lines collapsed to one
- [ ] Add test: idempotency with blank lines
- [ ] Verify existing tests still pass

### Task 6: Fix flow-to-block sequence indentation

Fix indentation when converting flow sequences to block style inside nested
structures (e.g., `command: ["python", ...]` inside a sequence item mapping).

The developer should:
1. Write a reproducing test with the exact K8s-style input from the bug report
2. Print/trace the Doc IR tree to see what indent levels are produced
3. Compare with the expected output to identify where indentation diverges
4. Fix the issue — likely in the interaction between `key_value_to_doc`
   (line 514-518), `sequence_item_to_doc` (line 547-560), and the printer's
   work-stack indent tracking

The fix may be in:
- `formatter.rs`: how `indent()` wraps are structured in nested contexts
- `rlsp-fmt/src/printer.rs`: how nested `Indent` nodes accumulate on the
  work stack

- [ ] Write reproducing test with K8s containers/command pattern
- [ ] Trace Doc IR and printer output to identify fault
- [ ] Fix indentation logic
- [ ] Add test: flow sequence in mapping in sequence item
- [ ] Add test: deeply nested flow-to-block conversion
- [ ] Verify all existing formatter tests still pass

### Task 7: Strip unnecessary quotes from Representation variants

After Task 4 (early_parse switch), quoted `Representation` variants preserve
their original style. Add logic to strip quotes when they're unnecessary.

In `node_to_doc` for `Representation(s, style, _tag)`, when `style` is
`SingleQuoted` or `DoubleQuoted`: check `needs_quoting(s)`. If false, emit
as plain `text(s.clone())` instead of quoting. If true, preserve original
quote style. This strips syntactic quotes (e.g., `"python"` from flow
context) while preserving semantic quotes (e.g., `"5000"`, `"true"`,
`"on"`).

- [ ] Add `needs_quoting` check for quoted Representation variants
- [ ] Add test: `"python"` in flow sequence → `python` in block
- [ ] Add test: `"5000"` stays quoted (looks like number)
- [ ] Add test: `"true"` stays quoted (boolean keyword)
- [ ] Add test: `"on"` stays quoted (YAML 1.1 keyword)
- [ ] Add test: `'hello'` single-quoted → `hello` plain (unnecessary)
- [ ] Verify idempotency after stripping

### Task 8: Add conformance test suite infrastructure

Add a conformance test module using the yaml-test-suite data and real-world
ecosystem fixtures to systematically discover bugs.

Integration approach:
- Clone/download yaml-test-suite data into `rlsp-yaml/tests/conformance/`
- The test suite has 320+ test cases with `in.yaml`, `test.event`,
  `in.json`, `out.yaml`, and `error` flag files
- Use file-driven tests (e.g., `datatest-stable` crate or manual directory
  iteration in a `#[test]` function)
- Focus on formatter round-trip tests: parse `in.yaml` → format → re-parse →
  verify structure matches `in.json` (semantic preservation)
- Focus on error tests: cases with `error` flag should produce diagnostics
- **Check how saphyr integrates yaml-test-suite** — saphyr already uses it
  in its own test suite, which may provide a proven integration pattern or
  even a reusable crate/module. Evaluate whether we can follow saphyr's
  approach before designing our own.
- Evaluate the best inclusion method (git submodule, vendored copy, or crate
  dependency) at implementation time based on what's actually available.

- [ ] Investigate saphyr's yaml-test-suite integration approach
- [ ] Download yaml-test-suite data files (method TBD based on investigation)
- [ ] Create `tests/conformance.rs` test module
- [ ] Implement formatter round-trip conformance tests
- [ ] Implement parser error conformance tests
- [ ] Document which tests pass/fail and why (saphyr limitations)

### Task 9: Add real-world ecosystem fixtures

Add hand-crafted test fixtures from real Kubernetes, GitHub Actions, and
Ansible files to test ecosystem-specific patterns the spec suite doesn't
cover.

- [ ] Add K8s fixtures: LimitRange, Deployment with containers/command,
      ConfigMap, Service with `status: {}`
- [ ] Add GitHub Actions fixtures: workflow with `on:`, flow sequences in
      `branches:`, matrix strategy
- [ ] Add Ansible fixtures: playbook with common patterns
- [ ] Wire fixtures into formatter round-trip tests
- [ ] Wire fixtures into validator tests (no false positives)

## Decisions

- **Preserve scalar style via `early_parse(false)`** — only in the formatter's
  parse call, not the shared parser. This isolates the change from schema
  validation, completions, and hover which need resolved values.
- **Strip quotes when `needs_quoting` returns false** — this reconciles
  "preserve original style" with "strip unnecessary quotes." Quotes that serve
  no semantic purpose are removed; quotes that prevent ambiguity are kept.
- **Suppress flow warnings for empty collections only** — non-empty flow
  collections still produce warnings. Empty `{}` and `[]` are idiomatic and
  the formatter itself preserves them.
- **Conformance suite uses yaml-test-suite + real-world fixtures** — the
  official suite covers spec compliance; real-world fixtures cover ecosystem
  patterns (K8s, GHA, Ansible).
- **Ecosystem-specific defaults are a separate follow-up** — correct base YAML
  behavior first, then tune per ecosystem using schema detection.
- **Flow/block toggle code action is a separate follow-up** — a new LSP
  feature, not a bug fix.
