---
test-name: block-to-flow-last-value-multiline-flow-mapping-comment-after
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The comment after the `}` of a last flow mapping stays behind the closing bracket.

## Test-Document

```yaml
a:
  b: 1
  c: {
    x: 1
    } # n
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: { x: 1 } } # n
d: 1
```
