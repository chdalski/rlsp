---
test-name: comment-duplicate-key-removed-sequence-item-mapping-moves-above-dash
category: comments
settings:
  format_remove_duplicate_keys: true
---

# Test: The first key of a sequence item prints its leading comments above the dash

The first key of a sequence item prints its leading comments above the dash.

## Test-Document

```yaml
- a: 1 # x
  a: 2
- b: 1 # y
  b: 2
```

## Expected-Document

```yaml
# x
- a: 2
# y
- b: 2
```
