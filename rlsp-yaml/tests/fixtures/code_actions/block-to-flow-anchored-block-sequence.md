---
test-name: block-to-flow-anchored-block-sequence
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Preserve anchor when converting anchored block sequence to flow

## Test-Document

```yaml
items: &mylist
  - a
  - b
```

## Expected-Document

```yaml
items: &mylist [a, b]
```
