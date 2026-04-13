---
test-name: structure-invalid-input-unchanged
category: structure
---

# Test: Invalid Input Returned Unchanged

An unclosed bracket `[bad` is invalid YAML. The formatter returns the input
unchanged rather than crashing or producing malformed output.

## Test-Document

```yaml
key: [bad
```

## Expected-Document

```yaml
key: [bad
```
