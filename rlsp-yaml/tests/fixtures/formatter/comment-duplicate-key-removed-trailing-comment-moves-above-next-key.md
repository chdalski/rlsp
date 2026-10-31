---
test-name: comment-duplicate-key-removed-trailing-comment-moves-above-next-key
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A removed entry hands its trailing comment to the next key that stays

A removed entry hands its trailing comment to the next key that stays.

## Test-Document

```yaml
a: 1 # x
b: 2
a: 3
```

## Expected-Document

```yaml
# x
b: 2
a: 3
```
