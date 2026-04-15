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

Note: the parser currently treats `? [a]: x` as outer-mapping{key=[a], value=x} rather
than the spec-correct outer-mapping{key=inner-mapping{key=[a], value=x}, value=""} (M2N8[1]
is in KNOWN_FAILURES). The expected output reflects what the formatter produces from the
current AST.

## Test-Document

```yaml
? [a]: x
```

## Expected-Document

```yaml
? - a
: x
```
