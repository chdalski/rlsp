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
- **Formatter: `single_quote` quotes keys unnecessarily** — `single_quote: true` wraps plain-safe mapping keys (e.g., `key` → `'key'`). Should only affect values. Discovered via fixture test spike (noted in `single-quote-option.md`). Being fixed in current plan `2026-04-14-formatter-bug-fixes.md`.
- **`preserve_quotes` formatter option** — add a `preserve_quotes: bool` setting (default `false`). When true, keep original quoting style on scalars even when `needs_quoting` returns false. Applies to both keys and values. Aligns with Prettier YAML's `quoteProps: "preserve"`. Low-effort — check is in `node_to_doc` Scalar branch where strip/keep is decided.
- **`formatIndentSequences` formatter option** — add a `formatIndentSequences: bool` setting (default `true`). When true (default), always produce indented block sequences (`script:\n  - item`). When false, produce indentless sequences (`script:\n- item`). Always normalize — no preserve mode. Formatter currently hardcodes indented style in `formatter.rs:658-669` via `indent()` wrapper.
- **Non-printable unicode character diagnostic** — Parser's comment lexer (`lexer/comment.rs`) and content scanning don't validate characters against `is_c_printable` (YAML 1.2 §5.1). Non-printable/control characters pass through silently. Add LSP diagnostic (`invalidCharacter`, Warning severity) for non-printable characters in comments and content. Security concern: invisible/homoglyph characters could hide malicious content. Parser should preserve them (no data loss); diagnostics should flag them.

## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
