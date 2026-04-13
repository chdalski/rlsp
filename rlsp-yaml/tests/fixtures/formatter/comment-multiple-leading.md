---
test-name: comment-multiple-leading
category: comments
---

# Test: Multiple Consecutive Leading Comments Stay Together

Two leading comment lines both appear before the key in their original order.

## Test-Document

```yaml
# line one
# line two
key: value
```

## Expected-Document

```yaml
# line one
# line two
key: value
```
