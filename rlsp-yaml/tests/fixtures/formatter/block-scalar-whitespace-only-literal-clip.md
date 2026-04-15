---
test-name: block-scalar-whitespace-only-literal-clip
category: block-scalar
---

# Test: Literal Block Scalar With Spaces-Only Content (Clip Chomp) Falls Back To Quoted

When a literal block scalar with default clip chomp (`|`) has a decoded value
that consists entirely of a space character and a newline, the formatter must
fall back to a double-quoted scalar. The guard triggers when any non-empty line
of the decoded value starts with a space and contains only whitespace characters.

## Test-Document

```yaml
key: |
  
```

## Expected-Document

```yaml
key: " \n"
```
