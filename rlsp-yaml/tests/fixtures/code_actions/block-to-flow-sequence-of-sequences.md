---
test-name: block-to-flow-sequence-of-sequences
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A sequence of sequences converts, and the comment after it stays.

## Test-Document

```yaml
k:
  - - a
    - b
  - - c
# n
z: 1
```

## Expected-Document

```yaml
k: [[a, b], [c]]
# n
z: 1
```
