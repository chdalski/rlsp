---
test-name: ecosystem-gha-blank-lines-preserved
category: ecosystem
---

# Test: GHA Blank Lines Preserved After Format

Blank lines between simple top-level keys (`name:`, `permissions:`, `env:`)
must be preserved after formatting.

## Test-Document

```yaml
name: CI

permissions:
  contents: read

env:
  COLOR: always
```

## Expected-Document

```yaml
name: CI

permissions:
  contents: read

env:
  COLOR: always
```
