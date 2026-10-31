---
test-name: comment-duplicate-key-removed-survivor-is-flow-collection-key
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A flow collection key that stays gets the moved comment above its entry

A flow collection key that stays gets the moved comment above its entry.

## Test-Document

```yaml
a: 1 # x
? [b, c]
: v
a: 2
```

## Expected-Document

```yaml
# x
? [b, c]
: v
a: 2
```
