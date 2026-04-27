---
test-name: block-scalar-preserve-anchor
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Anchor on the scalar value is preserved in the block scalar output

## Test-Document

```yaml
description: &myanchor "this is a long string that exceeds forty characters"
```

## Expected-Document

```yaml
description: &myanchor &myanchor |
  this is a long string that exceeds forty characters
```
