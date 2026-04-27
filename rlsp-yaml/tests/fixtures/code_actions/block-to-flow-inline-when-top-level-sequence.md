---
test-name: block-to-flow-inline-when-top-level-sequence
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Emit flow inline (no leading newline) when block sequence is a top-level mapping value

## Test-Document

```yaml
items:
  - one
  - two
```

## Expected-Document

```yaml
items: [one, two]
```
