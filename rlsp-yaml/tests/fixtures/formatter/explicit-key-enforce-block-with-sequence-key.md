---
test-name: explicit-key-enforce-block-with-sequence-key
category: explicit-key
settings:
  format_enforce_block_style: true
---

# Test: `enforce_block_style` Prints a Key That Holds an Explicit Entry in Block Form

When `format_enforce_block_style: true` and the key is a mapping with a flow
sequence key (`[a]: x`), the block form of the key nests an explicit `? `
entry inside the explicit `? ` of the outer key. The parser reads the `:` after
such a key as the value of that key, so the output loads as the same structure
and is stable on every format. The nested key `[a]` was written in flow style
and stays flow; the explicit key prefix `?` is preserved on both levels.
Exercises the interaction between `format_enforce_block_style` and
`needs_explicit_key`.

The parser produces: outer-mapping{key=inner-mapping{key=[a], value=x}, value=""}.

## Test-Document

```yaml
? [a]: x
```

## Expected-Document

```yaml
? ? [a]
  : x
:
```
