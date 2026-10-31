---
test-name: comment-duplicate-key-removed-two-keys-go-to-first-survivor
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Comments of removed entries with different keys gather at the first key that stays

Comments of removed entries with different keys gather at the first key that stays.

## Test-Document

```yaml
a: 1 # one
b: 2 # two
a: 3
b: 4
```

## Expected-Document

```yaml
# one
# two
a: 3
b: 4
```
