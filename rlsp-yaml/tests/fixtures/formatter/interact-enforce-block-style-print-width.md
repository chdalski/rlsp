---
test-name: interact-enforce-block-style-print-width
category: interaction
settings:
  format_enforce_block_style: true
  print_width: 10
---

# Test: format_enforce_block_style + print_width — Block Style Ignores Width for Collections

When `format_enforce_block_style: true`, flow sequences and mappings are
converted to block style. Block-style collections are not subject to the
`print_width` limit — they are always emitted as block regardless of line
length.

With `print_width: 10`, a flow sequence `[a, b, c]` would ordinarily stay as
a single-line flow form if it fit (or break across lines if it did not). With
block enforcement, it becomes a block sequence regardless, with each item on
its own line.

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
