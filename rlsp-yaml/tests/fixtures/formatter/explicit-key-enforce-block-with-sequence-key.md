---
test-name: explicit-key-enforce-block-with-sequence-key
category: explicit-key
settings:
  format_enforce_block_style: true
---

# Test: `enforce_block_style` Converts Flow Sequence Key to Block

When `format_enforce_block_style: true` and the key is a non-empty flow
sequence, the key is converted to block `- item` form. The explicit key
prefix `?` is preserved. Exercises the interaction between
`format_enforce_block_style` and `needs_explicit_key`.

## Test-Document

```yaml
? [a]: x
```

## Expected-Document

```yaml
? - a
:
  x: 
```
