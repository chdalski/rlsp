---
test-name: block-to-flow-next-sibling-is-sequence-item
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The next item of the enclosing sequence and the comment before it stay.

## Test-Document

```yaml
- k:
    b: 1
    c: 2
  # n
- y
```

## Expected-Document

```yaml
- k: { b: 1, c: 2 }
  # n
- y
```
