---
test-name: comment-duplicate-key-removed-nested-mapping
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: In a nested mapping the comment lands in the same mapping

In a nested mapping the comment lands in the same mapping.

## Test-Document

```yaml
o:
  a: 1 # x
  a: 2
p: 3
```

## Expected-Document

```yaml
o:
  # x
  a: 2
p: 3
```
