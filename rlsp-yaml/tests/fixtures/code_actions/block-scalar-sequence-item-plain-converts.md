---
test-name: block-scalar-sequence-item-plain-converts
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Convert long plain scalar sequence item to block scalar

## Test-Document

```yaml
- this is a very long plain scalar sequence item that exceeds forty chars
```

## Expected-Document

```yaml
- |
  this is a very long plain scalar sequence item that exceeds forty chars
```
