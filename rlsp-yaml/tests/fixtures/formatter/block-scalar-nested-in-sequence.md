---
test-name: block-scalar-nested-in-sequence
category: block-scalar
---

# Test: Block Scalar Nested in Sequence Item

A block scalar as a direct sequence item value (not inside a mapping-in-sequence).
The sequence is indented one level under `commands:`, and the fix adds another
level of indentation inside `repr_block_to_doc`, giving content lines 4 spaces.

## Test-Document

```yaml
commands:
  - |
    first
    second
```

## Expected-Document

```yaml
commands:
  - |
    first
    second
```
