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

- **Duplicate key enforcement levels** — Same pattern as flow style: add `duplicateKeys` severity setting (`"off"`/`"warning"`/`"error"`) and `formatRemoveDuplicateKeys` auto-fix toggle (keeps last occurrence per YAML spec).
- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.

## Open: rlsp-yaml-parser

- create bindings for python - https://pyo3.rs and typescript (wasm)
