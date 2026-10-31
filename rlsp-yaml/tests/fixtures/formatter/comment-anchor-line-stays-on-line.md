---
test-name: comment-anchor-line-stays-on-line
category: comments
---

# Test: A Comment After An Anchor On A Key's Line Stays On It

The anchor stays with the key's line and the comment follows it.

## Test-Document

```yaml
a: &x # c
  b: 1
```

## Expected-Document

```yaml
a: &x  # c
  b: 1
```
