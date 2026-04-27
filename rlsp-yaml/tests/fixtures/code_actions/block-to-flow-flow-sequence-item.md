---
test-name: block-to-flow-flow-sequence-item
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Handle flow sequence item when converting block sequence to flow

## Test-Document

```yaml
args:
  - [nested]
  - safe
```

## Expected-Document

```yaml
args: [[nested], safe]
```
