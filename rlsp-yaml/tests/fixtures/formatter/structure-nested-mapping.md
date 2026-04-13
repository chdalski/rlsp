---
test-name: structure-nested-mapping
category: structure
---

# Test: Nested Mapping

A child key indented under its parent is preserved with correct indentation.

## Test-Document

```yaml
parent:
  child: value
```

## Expected-Document

```yaml
parent:
  child: value
```
