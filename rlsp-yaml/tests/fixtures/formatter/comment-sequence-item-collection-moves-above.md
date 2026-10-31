---
test-name: comment-sequence-item-collection-moves-above
category: comments
---

# Test: A Comment On The Dash Line Of A Collection Item Moves Above The Dash

The data stays the same: a comment between the dash and the item's first entry reads as a comment of the collection, so it is printed above the dash, where it reads the same way when formatted again. A comment stays on its line when the output has that line and a slot for it; otherwise it moves to its own line above the node's line.

## Test-Document

```yaml
- # c
  a: 1
```

## Expected-Document

```yaml
# c
- a: 1
```
