---
test-name: comment-hash-inside-quoted-string
category: comments
---

# Test: Hash Inside Quoted String Not Treated as Comment

A `#` character inside a double-quoted string is not extracted as a comment.
The quoted string value is preserved intact.

## Test-Document

```yaml
key: "value # not a comment"
```

## Expected-Document

```yaml
key: "value # not a comment"
```
