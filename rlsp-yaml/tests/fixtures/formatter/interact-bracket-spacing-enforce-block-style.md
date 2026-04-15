---
test-name: interact-bracket-spacing-enforce-block-style
category: interaction
settings:
  bracket_spacing: false
  format_enforce_block_style: true
---

# Test: bracket_spacing + format_enforce_block_style — Spacing Is No-Op When Block Enforced

When `format_enforce_block_style: true`, flow mappings and sequences are
converted to block style — braces and brackets are never emitted. As a result,
`bracket_spacing` has no effect on the output: both `bracket_spacing: true`
and `bracket_spacing: false` produce the same block-style result.

This fixture sets `bracket_spacing: false` (non-default) alongside
`format_enforce_block_style: true` to confirm the block conversion takes
precedence and `bracket_spacing` is irrelevant.

## Test-Document

```yaml
meta: {a: 1, b: 2}
```

## Expected-Document

```yaml
meta:
  a: 1
  b: 2
```
