---
test-name: block-scalar-preserve-user-tag
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: User tag on the scalar value is preserved once in the block scalar output

## Test-Document

```yaml
description: !mytag "this is a long string that exceeds forty characters"
```

## Expected-Document

```yaml
description: !mytag |
  this is a long string that exceeds forty characters
```
