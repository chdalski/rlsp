---
test-name: comment-enforce-block-flow-item-moves-above-dash
category: comments
settings:
  format_enforce_block_style: true
---

# Test: A Comment After A Flow Item That Becomes A Block Collection Moves Above The Dash

With `format_enforce_block_style` the item is a block sequence. A comment on the dash line reads as a comment of the collection, so it moves above the dash. A comment stays on its line when the output has that line and a slot for it; otherwise it moves to its own line above the node's line.

## Test-Document

```yaml
- [1, 2] # c
```

## Expected-Document

```yaml
# c
- 
  - 1
  - 2
```
