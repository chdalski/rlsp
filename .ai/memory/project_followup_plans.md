---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

## Open: rlsp-fmt

## Open: rlsp-yaml

- **Flow style enforcement levels** — RedHat can forbid flow style (ERROR), we only warn. Add a severity setting on existing flowMap/flowSeq diagnostics.
- **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.

## Open: rlsp-yaml-parser