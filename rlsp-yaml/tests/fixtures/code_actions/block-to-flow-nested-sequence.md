---
test-name: block-to-flow-nested-sequence
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Convert block sequence of block sequences to flow style recursively

## Test-Document

```yaml
items:
  - - a
    - b
  - - c
    - d
```

## Expected-Document

```yaml
items: [[a, b], [c, d]]
```
