---
test-name: block-scalar-sequence-item-single-quoted-converts
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: Convert long single-quoted sequence item to block scalar

## Test-Document

```yaml
- 'this is a very long sequence item value that exceeds forty characters'
```

## Expected-Document

```yaml
- |
  this is a very long sequence item value that exceeds forty characters
```
