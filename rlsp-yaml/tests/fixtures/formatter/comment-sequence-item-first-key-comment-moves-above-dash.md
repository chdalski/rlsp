---
test-name: comment-sequence-item-first-key-comment-moves-above-dash
category: comments
---

# Test: The Key Comment Of A Sequence Item's First Entry Moves Above The Dash

The key's comment must leave the entry because the value has one. Above the entry it would sit between the dash and the entry, which reads as a comment of the item, so it moves above the dash. A comment stays on its line when the output has that line and a slot for it; otherwise it moves to its own line above the node's line.

## Test-Document

```yaml
- k: # c
    v # d
```

## Expected-Document

```yaml
# c
- k: v  # d
```
