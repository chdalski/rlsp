---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

## Open: rlsp-fmt

## Open: rlsp-yaml

- ~~**Flow style enforcement levels**~~ — Done (2026-04-13). Plan: `.ai/plans/2026-04-13-flow-style-preservation-and-enforcement.md`. Added `flowStyle` severity setting and `formatEnforceBlockStyle` toggle. Formatter now preserves flow style by default.
- **Duplicate key enforcement levels** — Same pattern as flow style: add `duplicateKeys` severity setting (`"off"`/`"warning"`/`"error"`) and `formatRemoveDuplicateKeys` auto-fix toggle (keeps last occurrence per YAML spec).
- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.

## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)