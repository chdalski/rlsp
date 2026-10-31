---
test-name: comment-explicit-key-joins-value-line
category: comments
---

# Test: A Comment On An Explicit Key's Line Joins The Entry's Line

The explicit key is printed as an implicit key, so the entry has one line and the comment takes its slot.

## Test-Document

```yaml
? k # c
: v
```

## Expected-Document

```yaml
k: v  # c
```
