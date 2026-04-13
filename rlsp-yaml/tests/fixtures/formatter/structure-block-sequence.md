---
test-name: structure-block-sequence
category: structure
---

# Test: Block Sequence Format

A block sequence under a mapping key uses `- item` format with correct indentation.

## Test-Document

```yaml
items:
  - one
  - two
  - three
```

## Expected-Document

```yaml
items:
  - one
  - two
  - three
```
