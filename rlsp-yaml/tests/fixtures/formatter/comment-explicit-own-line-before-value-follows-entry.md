---
test-name: comment-explicit-own-line-before-value-follows-entry
category: comments
---

# Test: A Comment Between An Explicit Key And Its Colon Follows The Entry

It produces the same events as the comment on the colon's line, so it prints the same way.

## Test-Document

```yaml
? k
# c
: v
```

## Expected-Document

```yaml
k: v
# c
```
