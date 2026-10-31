---
test-name: comment-duplicate-key-removed-inner-duplicates-of-removed-value-all-kept
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: Duplicate keys inside a removed value keep their comments too

Duplicate keys inside a removed value keep their comments too.

## Test-Document

```yaml
a:
  x: 1 # c1
  x: 2 # c2
b: 1
a: 3
```

## Expected-Document

```yaml
# c1
# c2
b: 1
a: 3
```
