---
test-name: comment-flow-collection-with-nested-comment-stays-flow-for-siblings
category: comments
---

# Test: Only The Flow Collections Above A Comment Change Style

A block collection cannot sit inside a flow collection, so every flow collection above a comment becomes block. The flow collection next to it, with no comment inside, stays flow.

## Test-Document

```yaml
[[1, 2], [ # c
  3]]
```

## Expected-Document

```yaml
- [1, 2]
# c
- 
  - 3
```
