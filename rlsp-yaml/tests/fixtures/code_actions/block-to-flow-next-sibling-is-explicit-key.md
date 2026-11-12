---
test-name: block-to-flow-next-sibling-is-explicit-key
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The next explicit key and the comment before it stay.

## Test-Document

```yaml
? k:
    b: 1
# n
? c
: d
```

## Expected-Document

```yaml
? k: { b: 1 }
# n
? c
: d
```
