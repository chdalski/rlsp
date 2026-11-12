---
test-name: block-to-flow-last-value-multiline-flow-sequence
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A last value that is a flow sequence over several lines ends the edit after its `]`.

## Test-Document

```yaml
a:
  b: 1
  c: [
    1,
    2
    ]
# n
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c: [1, 2] }
# n
d: 1
```
