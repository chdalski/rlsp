---
test-name: tag-core-schema-preserved-on-empty-scalar-mapping-key
category: tag
---

# Test: Core Schema Tag Preserved on Empty Scalar Mapping Key

When a mapping key is an empty scalar with a core schema tag (`!!null`, etc.),
the tag must be preserved in the output. A space before the `:` separator is
required to prevent the colon from being parsed as part of the tag URI suffix.

## Test-Document

```yaml
!!null : a
```

## Expected-Document

```yaml
!!null : a
```
