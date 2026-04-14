---
test-name: tag-core-schema-preserved-on-both-key-and-value-empty
category: tag
---

# Test: Core Schema Tag Preserved on Tagged Empty Scalar Key With Value

When a mapping key is an empty scalar with a core schema tag and the value is
a non-empty scalar, the key tag must be preserved with a space before `:`.
This covers the case where the key ends with tag characters that could merge
with the colon separator.

## Test-Document

```yaml
!!str : value
```

## Expected-Document

```yaml
!!str : value
```
