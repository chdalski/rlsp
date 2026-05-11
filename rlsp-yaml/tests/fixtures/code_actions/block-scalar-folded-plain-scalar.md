---
test-name: block-scalar-folded-plain-scalar
category: block-scalar
cursor: 0:0
applies-action: folded
---

# Test: Convert a long plain scalar mapping value to folded block scalar

## Test-Document

```yaml
description: this is a very long plain scalar value that exceeds forty chars
```

## Expected-Document

```yaml
description: >
  this is a very long plain scalar value that exceeds forty chars
```
