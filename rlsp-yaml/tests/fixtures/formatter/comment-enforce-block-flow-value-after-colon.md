---
test-name: comment-enforce-block-flow-value-after-colon
category: comments
settings:
  format_enforce_block_style: true
---

# Test: A Comment After A Flow Value That Becomes A Block Collection Follows The Colon

With `format_enforce_block_style` the value is a block sequence, so the line the comment was on is the key's line, and the comment stays after the colon.

## Test-Document

```yaml
a: [1, 2] # c
```

## Expected-Document

```yaml
a:  # c
  - 1
  - 2
```
