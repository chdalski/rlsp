---
test-name: tag-custom-on-sequence
category: tag
---

# Test: Custom Tag on a Flow Sequence

A custom (local) tag on a flow sequence value is preserved in the formatter
output. The tag appears before the flow sequence content, separated by a space.

## Test-Document

```yaml
key: !custom [a, b]
```

## Expected-Document

```yaml
key: !custom [a, b]
```
