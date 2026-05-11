---
test-name: block-scalar-folded-sequence-item-converts
category: block-scalar
cursor: 0:0
applies-action: folded
---

# Test: Convert long plain scalar sequence item to folded block scalar

## Test-Document

```yaml
- this is a very long plain scalar sequence item that exceeds forty chars
```

## Expected-Document

```yaml
- >
  this is a very long plain scalar sequence item that exceeds forty chars
```
