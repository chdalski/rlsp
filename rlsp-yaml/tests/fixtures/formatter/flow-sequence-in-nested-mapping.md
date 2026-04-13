---
test-name: flow-sequence-in-nested-mapping
category: flow-style
---

# Test: Flow Sequence Inside Nested Mapping Is Preserved

A flow sequence nested inside a mapping that is itself nested retains flow style.
Safe strings have quotes stripped; the sequence brackets are preserved.

## Test-Document

```yaml
job:
  run:
    command: ["echo", "hello"]
```

## Expected-Document

```yaml
job:
  run:
    command: [echo, hello]
```
