---
test-name: comment-duplicate-key-removed-comments-accumulate-across-removed-entries
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Removed entries that do not follow each other hand their comments to the same key

Removed entries that do not follow each other hand their comments to the same key.

## Test-Document

```yaml
a: 1 # x
b: 1
a: 2 # y
b: 2 # z
a: 3
```

## Expected-Document

```yaml
# x
# y
b: 2  # z
a: 3
```
