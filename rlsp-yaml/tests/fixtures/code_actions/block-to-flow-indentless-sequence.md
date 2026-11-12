---
test-name: block-to-flow-indentless-sequence
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A sequence written at the indent of its key converts, and the comment after it stays.

## Test-Document

```yaml
a:
- x
- y
# n
b: 1
```

## Expected-Document

```yaml
a: [x, y]
# n
b: 1
```
