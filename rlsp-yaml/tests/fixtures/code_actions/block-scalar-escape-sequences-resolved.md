---
test-name: block-scalar-escape-sequences-resolved
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: Double-quoted escape sequences (\n, \t, \\, \") are resolved in the block scalar output

## Test-Document

```yaml
summary: "line one\nline two\ttabbed\\backslash\"quote and more padding here"
```

## Expected-Document

```yaml
summary: |
  line one
  line two	tabbed\backslash"quote and more padding here
```
