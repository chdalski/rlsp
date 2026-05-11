---
test-name: block-scalar-sequence-item-nested-in-mapping-converts
category: block-scalar
cursor: 1:2
applies-action: block scalar
---

# Test: Convert long sequence item nested inside a mapping value to block scalar

## Test-Document

```yaml
commands:
  - "this is a very long sequence item value that exceeds forty characters"
```

## Expected-Document

```yaml
commands:
  - |
    this is a very long sequence item value that exceeds forty characters
```
