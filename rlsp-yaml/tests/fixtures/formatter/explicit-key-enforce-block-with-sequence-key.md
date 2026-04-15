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

The parser produces: outer-mapping{key=inner-mapping{key=[a], value=x}, value=""}.
The formatter outputs the inner mapping as a nested explicit key with the
sequence converted to block form.

Note: the formatter does not yet produce idempotent output for this case
(M2N8[1] is in formatter KNOWN_FAILURES). This fixture verifies the formatter
does not panic or corrupt the output on the first formatting pass.

## Test-Document

```yaml
? [a]: x
```

## Expected-Document

```yaml
? ? - a
  : x
:
```
