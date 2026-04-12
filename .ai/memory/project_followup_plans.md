---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

## Open — Feature Work

1. **YAML version selection** — `yaml.yamlVersion` for 1.1 vs 1.2 boolean interpretation (`on`/`off`/`yes`/`no`)
2. **Flow style enforcement levels** — RedHat can forbid flow style (ERROR), we only warn. Add a severity setting on existing flowMap/flowSeq diagnostics.
3. **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.

## Completed — Cleanup queue (2026-04-12)

All validated items delivered in plan `2026-04-12-cleanup-queue.md`:
- **C1** — stale line-number references rewritten (commit `10be323`)
- **C2** — `parse_block_header` if/else converted to match (commit `10be323`)
- **C3** — `PlainScalarKind` enum + `classify_plain_scalar` extracted (commit `6569e1c`)
- **C4a** — 5 newline-push loops replaced with `repeat_n` (commit `10be323`)

Items investigated and excluded: C2 (color.rs, line_mapping.rs, scalar_helpers.rs, server.rs — not match-convertible), C4b (semantically incorrect), C4c (side-effect incompatible), C4d (complex branch doesn't fit `matches!`).
