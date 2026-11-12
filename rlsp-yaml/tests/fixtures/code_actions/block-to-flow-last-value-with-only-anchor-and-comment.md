---
test-name: block-to-flow-last-value-with-only-anchor-and-comment
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The comment on the line of a last value that holds only an anchor stays.

## Test-Document

```yaml
a:
  b: 1
  c: &x # n
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: &x } # n
d: 1
```
