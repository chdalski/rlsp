---
test-name: comment-flow-sequence-becomes-block
category: comments
---

# Test: A Flow Sequence That Holds A Comment Becomes A Block Sequence

Flow style has no slot for a comment, so a flow sequence with a comment inside is printed in block style, and the comment is kept above the item it preceded. The output reads the same data.

## Test-Document

```yaml
a: [
  # c
  1
  ]
```

## Expected-Document

```yaml
a:
  # c
  - 1
```
