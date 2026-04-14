---
test-name: tag-core-schema-stripped-on-non-empty-scalar
category: tag
---

# Test: Core Schema Tag Still Stripped on Non-Empty Scalar

Core schema tags on non-empty scalars continue to be stripped. The tag
preservation for empty scalars must not affect the normal stripping behavior
for scalars that have a value.

## Test-Document

```yaml
value: !!str hello
```

## Expected-Document

```yaml
value: hello
```
