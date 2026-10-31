---
test-name: comment-duplicate-key-removed-survivor-with-anchor-key
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A key with an anchor that stays takes the comment above the anchor

A key with an anchor that stays takes the comment above the anchor.

## Test-Document

```yaml
a: 1 # x
&k b: 2
a: 3
```

## Expected-Document

```yaml
# x
&k b: 2
a: 3
```
