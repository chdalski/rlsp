---
test-name: block-scalar-preserve-anchor-and-user-tag
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: Anchor and user tag on the scalar value are each preserved once in the block scalar output

## Test-Document

```yaml
description: &a !mytag "this is a long string that exceeds forty characters"
```

## Expected-Document

```yaml
description: &a !mytag |
  this is a long string that exceeds forty characters
```
