---
test-name: blank-line-inside-block-scalar-unaffected
category: blank-lines
---

# Test: Blank Lines Inside Block Scalar Are Not Collapsed

A blank line inside a literal block scalar (`|`) is part of the scalar content
and is preserved as-is by the formatter.

Note: the formatter does not re-indent block scalar content; it is emitted
verbatim. The output indentation is normalised by the block scalar renderer.

## Test-Document

```yaml
body: |
  line one

  line three
```

## Expected-Document

```yaml
body: |
line one


line three
```
