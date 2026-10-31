---
test-name: comment-duplicate-key-removed-flow-sequence-value-comment
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: A comment inside the flow sequence value of a removed entry moves

A comment inside the flow sequence value of a removed entry moves.

## Test-Document

```yaml
a: [1, # c
  2]
b: 0
a: 3
```

## Expected-Document

```yaml
# c
b: 0
a: 3
```
