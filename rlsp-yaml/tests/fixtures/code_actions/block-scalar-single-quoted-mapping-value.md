---
test-name: block-scalar-single-quoted-mapping-value
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: Convert a long single-quoted scalar mapping value to block scalar

## Test-Document

```yaml
description: 'this is a very long single-quoted scalar that exceeds forty chars'
```

## Expected-Document

```yaml
description: |
  this is a very long single-quoted scalar that exceeds forty chars
```
