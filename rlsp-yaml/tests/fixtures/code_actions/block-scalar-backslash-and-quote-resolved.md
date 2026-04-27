---
test-name: block-scalar-backslash-and-quote-resolved
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Double-quoted \\ and \" escape sequences resolve to literal backslash and double-quote

## Test-Document

```yaml
key: "contains \\backslash and \"quote\" here plus some extra padding chars"
```

## Expected-Document

```yaml
key: |
  contains \backslash and "quote" here plus some extra padding chars
```
