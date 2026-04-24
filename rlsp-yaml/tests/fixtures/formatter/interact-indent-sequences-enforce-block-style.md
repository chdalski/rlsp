---
test-name: interact-indent-sequences-enforce-block-style
category: interaction
settings:
  format_enforce_block_style: true
  format_indent_sequences: false
---

# Test: enforce_block_style + indent_sequences: false Interaction

When `format_enforce_block_style: true` and `format_indent_sequences: false` are
both set, flow sequences are first converted to block style (by `enforce_block_style`)
and the resulting block sequences are then rendered indentless (by `indent_sequences:
false`). Both settings take effect in the same formatting pass.

## Test-Document

```yaml
items: [a, b, c]
```

## Expected-Document

```yaml
items:
- a
- b
- c
```
