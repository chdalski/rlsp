---
test-name: comment-duplicate-key-removed-survivor-keeps-own-trailing-comment
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: The key that stays keeps its own trailing comment

The key that stays keeps its own trailing comment.

## Test-Document

```yaml
a: 1 # x
b: 2
a: 3 # keep
```

## Expected-Document

```yaml
# x
b: 2
a: 3  # keep
```
