---
test-name: block-scalar-sequence-item-double-quoted-converts
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Convert long double-quoted sequence item to block scalar

## Test-Document

```yaml
- "this is a very long sequence item value that exceeds forty characters"
```

## Expected-Document

```yaml
- |
  this is a very long sequence item value that exceeds forty characters
```
