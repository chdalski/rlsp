---
test-name: comment-explicit-key-and-value-key-comment-moves-above
category: comments
---

# Test: An Explicit Key's Comment Moves Above The Entry When The Value Has A Comment

The entry has one line and one slot, which the value's comment takes. The key's comment moves to its own line above the entry. A comment stays on its line when the output has that line and a slot for it; otherwise it moves to its own line above the node's line.

## Test-Document

```yaml
? k # c
: v # d
```

## Expected-Document

```yaml
# c
k: v  # d
```
