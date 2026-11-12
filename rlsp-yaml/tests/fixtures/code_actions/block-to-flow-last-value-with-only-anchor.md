---
test-name: block-to-flow-last-value-with-only-anchor
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A last value that holds only an anchor ends the edit right after the anchor.

## Test-Document

```yaml
a:
  b: 1
  c: &x
# n
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: &x }
# n
d: 1
```
