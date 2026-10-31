---
test-name: comment-key-and-value-key-comment-moves-above
category: comments
---

# Test: A Key's Comment Moves Above The Entry When The Value Has A Comment

The output joins the key and the value on one line, and that line has one slot, which the value's comment takes. The comment of the key moves to its own line above the entry. A comment stays on its line when the output has that line and a slot for it; otherwise it moves to its own line above the node's line.

## Test-Document

```yaml
k: # c
  v # d
```

## Expected-Document

```yaml
# c
k: v  # d
```
