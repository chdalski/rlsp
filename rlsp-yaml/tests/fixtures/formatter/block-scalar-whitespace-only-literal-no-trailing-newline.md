---
test-name: block-scalar-whitespace-only-literal-no-trailing-newline
category: block-scalar
---

# Test: Literal Block Scalar With Whitespace-Only Content Line And Strip Chomp

When a literal block scalar uses strip chomp (`|-`) and has a content line
consisting solely of spaces at the block indent level, the YAML parser treats
it as a blank line. Blank-only content with strip chomp produces an empty decoded
value. The formatter preserves the block scalar style with strip indicator and
no content lines.

## Test-Document

```yaml
key: |-
  
```

## Expected-Document

```yaml
key: |-
```
