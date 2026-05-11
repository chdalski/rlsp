---
test-name: block-scalar-folded-preserve-anchor
category: block-scalar
cursor: 0:0
applies-action: folded
---

# Test: Anchor on the scalar value is preserved once in the folded block scalar output

## Test-Document

```yaml
description: &myanchor "this is a long string that exceeds forty characters"
```

## Expected-Document

```yaml
description: &myanchor >
  this is a long string that exceeds forty characters
```
