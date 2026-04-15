---
test-name: block-scalar-whitespace-only-literal-clip
category: block-scalar
---

# Test: Literal Block Scalar With Whitespace-Only Content Line Strips To Empty

When a literal block scalar with default clip chomp (`|`) has a content line
consisting solely of spaces at the block indent level, the YAML parser treats
it as a blank line. Blank-only content with clip chomp produces an empty decoded
value. The formatter preserves the block scalar style with no content lines.

## Test-Document

```yaml
key: |
  
```

## Expected-Document

```yaml
key: |
```
