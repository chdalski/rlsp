---
test-name: comment-duplicate-key-removed-survivor-is-alias-key
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: The key that stays is not the alias key of the removed entry

The key that stays is not the alias key of the removed entry.

## Test-Document

```yaml
b: 0
*r : 1 # x
c: 1
*r : 2
```

## Expected-Document

```yaml
b: 0
# x
c: 1
*r : 2
```
