---
test-name: flow-sequence-deeply-nested
category: flow-style
---

# Test: Flow Sequence Three Levels Deep Is Preserved

A flow sequence four levels deep (inside a sequence item mapping, inside a
sequence, inside nested mappings) retains flow style.

## Test-Document

```yaml
jobs:
  build:
    steps:
      - name: run
        run: ["bash", "-c", "echo hi"]
```

## Expected-Document

```yaml
jobs:
  build:
    steps:
      - name: run
        run: [bash, "-c", echo hi]
```
