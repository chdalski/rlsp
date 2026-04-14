---
test-name: block-scalar-tab-only-content-line
category: block-scalar
---

# Test: Literal Block Scalar With Whitespace-Only Content Line Uses Explicit Indent Indicator

When a literal block scalar's decoded value consists entirely of whitespace
(spaces and/or tabs), the YAML parser cannot reliably auto-detect the block
indentation level. An explicit indentation indicator digit (`|2`, `|2-`, etc.)
is emitted so the re-parser uses the correct indent, preventing a spurious
leading space from being prepended to the value.

The explicit indicator ensures idempotent round-tripping.

## Test-Document

```yaml
foo: |
 	
bar: 1
```

## Expected-Document

```yaml
foo: |2
  	
bar: 1
```
