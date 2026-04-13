---
test-name: structure-no-blank-lines-not-added
category: structure
---

# Test: No Blank Lines Are Not Added

A two-key mapping without blank lines is preserved as-is — the formatter does
not inject blank lines between keys.

## Test-Document

```yaml
a: 1
b: 2
```

## Expected-Document

```yaml
a: 1
b: 2
```
