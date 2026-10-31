---
test-name: comment-duplicate-key-removed-adjacent-survivor-is-the-duplicate
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Comments of two removed entries land on the last occurrence

Comments of two removed entries land on the last occurrence.

## Test-Document

```yaml
a: 1 # x
a: 2 # y
a: 3 # z
b: 0
```

## Expected-Document

```yaml
# x
# y
a: 3  # z
b: 0
```
