---
test-name: comment-duplicate-key-removed-comment-in-flow-mapping-turns-it-block
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A flow mapping that receives a moved comment becomes a block mapping

A flow mapping that receives a moved comment becomes a block mapping.

## Test-Document

```yaml
x: {a: 1, # c
 b: 2, a: 3}
y: 1
```

## Expected-Document

```yaml
x:
  # c
  b: 2
  a: 3
y: 1
```
