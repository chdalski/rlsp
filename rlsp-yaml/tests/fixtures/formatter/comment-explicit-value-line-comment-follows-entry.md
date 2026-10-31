---
test-name: comment-explicit-value-line-comment-follows-entry
category: comments
---

# Test: A Comment On The Line Of An Explicit Entry's Colon Follows The Entry

The loader stores this comment, and the one on a line of its own between the key and the colon, as a comment after the entry, so both print as a comment line below it.

## Test-Document

```yaml
? k
: # c
  v
```

## Expected-Document

```yaml
k: v
# c
```
