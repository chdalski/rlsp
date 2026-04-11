---
name: Follow-up task queue
description: Remaining items after parser implementation, conformance hardening, migration, and workaround removal
type: project
---

## Open — Feature Work

1. **YAML version selection** — `yaml.yamlVersion` for 1.1 vs 1.2 boolean interpretation (`on`/`off`/`yes`/`no`)
2. **Flow style enforcement levels** — RedHat can forbid flow style (ERROR), we only warn. Add a severity setting on existing flowMap/flowSeq diagnostics.
3. **Custom tag type annotations** — RedHat's customTags supports `!include scalar`, `!ref mapping` type annotations. Ours is a plain string allowlist — add type annotation support.
