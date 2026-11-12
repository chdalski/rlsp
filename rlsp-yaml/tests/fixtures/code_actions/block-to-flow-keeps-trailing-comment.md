---
test-name: block-to-flow-keeps-trailing-comment
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The comment on the line of the last entry stays behind the closing bracket.

## Test-Document

```yaml
a:
  b: 1
  c: 2 # note
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: 2 } # note
d: 1
```
