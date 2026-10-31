---
test-name: comment-sequence-item-quoted-stays-on-line
category: comments
---

# Test: A Comment After A Quoted Sequence Item Stays On Its Line

The quotes stay because the value would otherwise load as a number; the comment keeps the line it was on.

## Test-Document

```yaml
- "1.32" # msrv
```

## Expected-Document

```yaml
- "1.32"  # msrv
```
