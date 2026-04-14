---
test-name: tag-core-schema-preserved-on-empty-scalar-sequence-item
category: tag
---

# Test: Core Schema Tag Preserved on Empty Scalar in Sequence

When a sequence item has a core schema tag (`!!str`, `!!null`, etc.) on an
empty scalar, the tag must be preserved in the output. The tag carries semantic
meaning that cannot be inferred from the absent value.

## Test-Document

```yaml
- !!str
- plain
```

## Expected-Document

```yaml
- !!str
- plain
```
