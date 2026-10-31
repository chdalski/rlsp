---
test-name: comment-duplicate-key-removed-comment-in-root-flow-mapping-turns-it-block
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A root flow mapping that receives a moved comment becomes a block mapping

A root flow mapping that receives a moved comment becomes a block mapping.

## Test-Document

```yaml
{a: 1, # c
 b: 2, a: 3}
```

## Expected-Document

```yaml
# c
b: 2
a: 3
```
