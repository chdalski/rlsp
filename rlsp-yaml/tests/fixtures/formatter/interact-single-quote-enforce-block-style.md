---
test-name: interact-single-quote-enforce-block-style
category: interaction
settings:
  single_quote: true
  format_enforce_block_style: true
---

# Test: single_quote + format_enforce_block_style — Block Conversion With Single-Quoted Values

When both `single_quote: true` and `format_enforce_block_style: true` are set,
flow sequences are converted to block sequences AND plain string values that do
not require quoting are wrapped in single quotes.

The combined effect: a flow sequence `[hello, world]` becomes a block sequence
where each value is single-quoted.

## Test-Document

```yaml
items: [hello, world]
```

## Expected-Document

```yaml
items:
  - 'hello'
  - 'world'
```
