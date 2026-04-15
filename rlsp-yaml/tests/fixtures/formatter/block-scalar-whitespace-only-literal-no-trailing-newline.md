---
test-name: block-scalar-whitespace-only-literal-no-trailing-newline
category: block-scalar
---

# Test: Literal Block Scalar With Spaces-Only Content And No Trailing Newline Falls Back To Quoted

When a literal block scalar uses strip chomp (`|-`) and the decoded value
consists solely of space characters (no trailing newline), the formatter must
fall back to a double-quoted scalar. The double-quoted form preserves the space
content faithfully without triggering the re-parser's blank-line indentation
constraint.

## Test-Document

```yaml
key: |-
  
```

## Expected-Document

```yaml
key: " "
```
