---
test-name: block-scalar-chomp-keep
category: block-scalar
---

# Test: Block Scalar Keep Chomping

A literal block scalar with keep chomping (`|+`). The keep indicator is
preserved verbatim and content lines are indented one level relative to the
parent key. Trailing-newline behavior is the parser's domain; the formatter
emits what str::lines() provides.

## Test-Document

```yaml
key: |+
  keep trailing
```

## Expected-Document

```yaml
key: |+
  keep trailing
```
