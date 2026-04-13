---
test-name: structure-nested-sequence-in-sequence
category: structure
---

# Test: Nested Sequence in Sequence

A sequence-in-sequence (using `- - item` syntax) is preserved. The inner
sequence items are indented under their parent sequence item.

## Test-Document

```yaml
outer:
  - - inner1
    - inner2
  - simple
```

## Expected-Document

```yaml
outer:
  - 
    - inner1
    - inner2
  - simple
```
