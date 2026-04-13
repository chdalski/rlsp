---
test-name: structure-deeply-nested
category: structure
---

# Test: Deeply Nested Mapping (3 Levels)

Three levels of mapping nesting are preserved with correct indentation at each level.

## Test-Document

```yaml
a:
  b:
    c: deep
```

## Expected-Document

```yaml
a:
  b:
    c: deep
```
