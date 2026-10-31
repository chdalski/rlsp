---
test-name: comment-flow-collection-with-own-comment-stays-flow
category: comments
---

# Test: A Comment After A Flow Collection Keeps It In Flow Style

Only a comment inside a flow collection changes its style. A comment after the closing bracket belongs to the collection itself, so the collection stays flow and the comment stays after it. A sibling flow collection without a comment stays flow as well.

## Test-Document

```yaml
a: [1] # c
b: [2]
```

## Expected-Document

```yaml
a: [1]  # c
b: [2]
```
