---
test-name: comment-after-document-end-moves-below
category: comments
---

# Test: Comment On The Document End Line Moves Below The Marker

A comment on the `...` line moves to its own line below the marker.

## Test-Document

```yaml
a: 1
... # c
```

## Expected-Document

```yaml
a: 1
...
# c
```
