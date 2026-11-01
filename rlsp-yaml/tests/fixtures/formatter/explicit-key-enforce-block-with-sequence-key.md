---
test-name: explicit-key-enforce-block-with-sequence-key
category: explicit-key
settings:
  format_enforce_block_style: true
---

# Test: `enforce_block_style` Keeps a Key That Holds an Explicit Entry Flow

When `format_enforce_block_style: true` and the key is a mapping with a flow
sequence key (`[a]: x`), the block form of the key would nest an explicit
`? ` entry inside the explicit `? ` of the outer key. The parser reads the `:`
after such a key as the start of a new entry, so the output would load as a
different structure and grow on every format. The key keeps the flow style
the option would otherwise remove; the explicit key prefix `?` is preserved.
Exercises the interaction between `format_enforce_block_style` and
`needs_explicit_key`.

The parser produces: outer-mapping{key=inner-mapping{key=[a], value=x}, value=""}.

## Test-Document

```yaml
? [a]: x
```

## Expected-Document

```yaml
? { [a]: x }
:
```
