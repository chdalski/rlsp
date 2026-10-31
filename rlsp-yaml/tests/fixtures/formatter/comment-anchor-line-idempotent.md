---
test-name: comment-anchor-line-idempotent
category: comments
idempotent: true
---

# Test: A Comment After An Anchor Is Stable

The comment after the anchor stays on the key's line when formatted again.

## Test-Document

```yaml
a: &x  # c
  b: 1
```
