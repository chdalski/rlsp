---
test-name: block-to-flow-quote-comma-item
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Quote sequence item containing comma when converting block sequence to flow

## Test-Document

```yaml
args:
  - a, b
  - c
```

## Expected-Document

```yaml
args: ["a, b", c]
```
