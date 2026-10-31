---
test-name: comment-flow-mapping-becomes-block
category: comments
---

# Test: A Flow Mapping That Holds A Comment Becomes A Block Mapping

Flow style has no slot for a comment, so a flow mapping with a comment inside is printed in block style, and the comment is kept above the key it preceded. The output reads the same data.

## Test-Document

```yaml
a: {
  # c
  b: 1
  }
```

## Expected-Document

```yaml
a:
  # c
  b: 1
```
