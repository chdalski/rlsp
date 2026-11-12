---
test-name: block-to-flow-next-sibling-is-explicit-value
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An explicit `:` value indicator after the collection stays.

## Test-Document

```yaml
? k:
    b: 1
# n
: v
```

## Expected-Document

```yaml
? k: { b: 1 }
# n
: v
```
