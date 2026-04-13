---
test-name: mixed-flow-sequence-in-block-mapping
category: flow-style
---

# Test: Flow Sequence Inside Block Mapping Is Preserved

A flow sequence nested as a value in a block mapping retains its flow style.
The block structure of the parent is not affected.

## Test-Document

```yaml
config:
  ports: [8080, 443]
```

## Expected-Document

```yaml
config:
  ports: [8080, 443]
```
