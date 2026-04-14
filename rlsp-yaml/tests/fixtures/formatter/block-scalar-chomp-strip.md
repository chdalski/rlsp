---
test-name: block-scalar-chomp-strip
category: block-scalar
---

# Test: Block Scalar Strip Chomping

A literal block scalar with strip chomping (`|-`). The strip indicator is
preserved verbatim and content lines are indented one level relative to the
parent key.

## Test-Document

```yaml
key: |-
  no trailing newline
```

## Expected-Document

```yaml
key: |-
  no trailing newline
```
