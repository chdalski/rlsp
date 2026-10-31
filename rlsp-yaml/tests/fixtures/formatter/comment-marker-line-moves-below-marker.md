---
test-name: comment-marker-line-moves-below-marker
category: comments
---

# Test: Comment On The Document Start Line Moves Below The Marker

A comment on the `---` line moves to its own line below the marker. The data stays the same, and a modeline written there becomes a line the server reads.

## Test-Document

```yaml
--- # c
a: 1
```

## Expected-Document

```yaml
---
# c
a: 1
```
