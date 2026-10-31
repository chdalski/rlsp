---
test-name: comment-duplicate-key-removed-comment-stays-in-source-order-with-neighbours
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: The comments of the removed entry precede the own leading comments of the key that stays

The comments of the removed entry precede the own leading comments of the key that stays.

## Test-Document

```yaml
k: 0
# la
a: 1 # x
# own
b: 2
a: 3
```

## Expected-Document

```yaml
k: 0
# la
# x
# own
b: 2
a: 3
```
