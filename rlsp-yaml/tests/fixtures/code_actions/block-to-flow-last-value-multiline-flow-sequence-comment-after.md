---
test-name: block-to-flow-last-value-multiline-flow-sequence-comment-after
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The comment after the `]` of a last flow sequence stays behind the closing bracket.

## Test-Document

```yaml
a:
  b: 1
  c: [
    1,
    2
    ] # n
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: [1, 2] } # n
d: 1
```
