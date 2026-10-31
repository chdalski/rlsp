---
test-name: comment-key-line-single-value-joins-line
category: comments
---

# Test: A Comment On A Key's Line Joins The Value That Moves Up To It

The value is on the line below the key in the source and on the key's line in the output. The comment's line has a slot after the value, so the comment stays on it. A comment stays on its line when the output has that line and a slot for it; otherwise it moves to its own line above the node's line.

## Test-Document

```yaml
k: # c
  v
```

## Expected-Document

```yaml
k: v  # c
```
