---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

<!-- Only track open items here. Completed work lives in its plan file
     and git history — duplicating it here adds noise and stale state.
     Remove items when their plan is marked Completed. -->

## Open: rlsp-fmt

## Open: rlsp-yaml

- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.
- **LSP lifecycle test rstest reduction** — ~34 tests in `lsp_lifecycle.rs` (3000 lines) follow repetitive patterns: "unknown doc returns null" (~8), diagnostic suppression (~10), flowStyle severity (3), max_items_computed (8), settings toggles (~5). Parameterize with rstest to reduce ~500-800 lines. Pure refactoring, no behavior change.
- **`formatIndentSequences` formatter option** — add a `formatIndentSequences: bool` setting (default `true`). When true (default), always produce indented block sequences (`script:\n  - item`). When false, produce indentless sequences (`script:\n- item`). Always normalize — no preserve mode. Formatter currently hardcodes indented style in `formatter.rs:658-669` via `indent()` wrapper.
- **Non-printable unicode character diagnostic** — Parser's comment lexer (`lexer/comment.rs`) and content scanning don't validate characters against `is_c_printable` (YAML 1.2 §5.1). Non-printable/control characters pass through silently. Add LSP diagnostic (`invalidCharacter`, Warning severity) for non-printable characters in comments and content. Security concern: invisible/homoglyph characters could hide malicious content. Parser should preserve them (no data loss); diagnostics should flag them.
- **Formatter fixture gaps: interacting settings combinations** — Fixtures test each formatter setting in isolation but no combinations. Add fixtures for interacting setting pairs (settings that affect the same formatting decision). Derive pairs from `YamlFormatOptions` in `formatter.rs`; see `tests/fixtures/formatter/CLAUDE.md` for guidance.
- **Expand `block_to_flow` code action to support nested block structures** — The action currently refuses nested inputs via `return None` in `code_actions.rs:420`. The `2026-04-18-retrofit-block-to-flow-code-action.md` plan preserved this narrow behavior to keep scope minimal (bug-class elimination, not feature expansion). After the retrofit lands, the AST+formatter path handles nesting automatically — lifting the restriction is cheap. Enhancement plan: remove the pre-check, add tests for nested block-to-flow conversions, confirm the formatter produces correct flow output (e.g., `{a: {b: 1}}`, `[[1, 2], [3, 4]]`).
- **Retrofit `quoted_bool_to_unquoted` to AST+formatter** — Currently span-local text replacement of `"true"` → `true` etc. AST pattern: find the `Node::Scalar`, clone with `style: ScalarStyle::Plain`, re-emit via `format_subtree`. Low complexity. Motivation: architectural consistency — bring all structural scalar-transform code actions under the "one parser, one AST" rule.
- **Retrofit `yaml11_bool_actions` to AST+formatter** — Currently text replacement of `yes`/`no`/`on`/`off` → quoted or converted form. AST pattern: change the scalar's `value` and/or `style`, re-emit via `format_subtree`. Low complexity. Same architectural-consistency motivation.
- **Retrofit `yaml11_octal_actions` to AST+formatter** — Currently text replacement of `0o12` → `10` (or similar). AST pattern: change scalar `value`, re-emit. Low complexity.
- **Retrofit `schema_yaml11_bool_type_actions` to AST+formatter** — Same shape as `yaml11_bool_actions`. Low complexity. Can possibly be combined with the `yaml11_bool_actions` retrofit plan since they share structure.
- **Retrofit `delete_unused_anchor` to AST+formatter** — Currently text replacement removing `&anchor_name` from a line. AST pattern: clone the node with `anchor: None`, re-emit via `format_subtree`. Low complexity; edge case is whether the span covers just the `&name` token or the whole node — the AST version emits the whole node, so the edit range is the node's `loc`.
- **Retrofit `string_to_block_scalar` to AST+formatter** — Currently text replacement converting quoted/plain scalars to block-scalar (`|` / `>`) form. AST pattern: clone `Node::Scalar` with `style: ScalarStyle::Literal` (or `Folded`), re-emit via `format_subtree` which handles block-scalar indentation semantics. Low-to-medium complexity (block scalars have indentation-sensitivity the other scalar styles don't). Preempts audit gap #3 from the queue (scheduled as "audit `string_to_block_scalar`" but upgrading it to direct retrofit is appropriate now that the AST pattern is proven).
- **`tab_to_spaces` stays as text replacement** — NOT a retrofit candidate. Tabs are a pre-parse lexical concern (YAML 1.2 §6.1 forbids them for indentation); the parser normalizes or rejects them, so they're not represented in the AST. `tab_to_spaces` is whitespace-cleanup before any structural editing applies. Belongs in the text-edit carve-out category alongside modelines and BOM. Documented here so future audits don't treat this as a missing retrofit.

## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
