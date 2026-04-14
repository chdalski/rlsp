---
test-name: tag-custom-on-mapping
category: tag
---

# Test: Custom Tag on a Flow Mapping

A custom (local) tag on a flow mapping value is preserved in the formatter
output. The tag appears before the flow mapping content, separated by a space.
Core schema tags (`!!map`, `!!seq`, `!!str`, etc.) are not preserved — only
user-defined tags like `!custom` are emitted.

## Test-Document

```yaml
key: !custom {a: 1}
```

## Expected-Document

```yaml
key: !custom { a: 1 }
```
