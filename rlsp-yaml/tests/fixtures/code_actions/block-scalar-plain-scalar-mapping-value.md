---
test-name: block-scalar-plain-scalar-mapping-value
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Convert a long plain scalar mapping value to block scalar

## Test-Document

```yaml
description: this is a very long plain scalar value that exceeds forty chars
```

## Expected-Document

```yaml
description: |
  this is a very long plain scalar value that exceeds forty chars
```
