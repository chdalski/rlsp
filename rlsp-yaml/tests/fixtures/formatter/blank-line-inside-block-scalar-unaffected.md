---
test-name: blank-line-inside-block-scalar-unaffected
category: blank-lines
---

# Test: Blank Lines Inside Block Scalar Are Not Collapsed

A blank line inside a literal block scalar (`|`) is part of the scalar content
and is preserved by the formatter. Content lines are indented one level
relative to the parent key. Blank lines within the scalar remain truly empty
(no trailing spaces are added).

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
