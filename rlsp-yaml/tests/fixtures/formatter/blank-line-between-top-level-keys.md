---
test-name: blank-line-between-top-level-keys
category: blank-lines
---

# Test: Blank Lines Between Top-Level Keys Preserved

Blank lines separating top-level keys are preserved after formatting.

## Test-Document

```yaml
on: push

permissions:
  contents: read

jobs:
  build: {}
```

## Expected-Document

```yaml
on: push

permissions:
  contents: read

jobs:
  build: {}
```
