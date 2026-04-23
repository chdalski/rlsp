---
test-name: tag-user-preserved-on-empty-scalar-sequence-item
category: tag
---

# Test: User Tag Preserved on Empty Scalar Sequence Item

A user-defined (non-core) tag on an empty scalar sequence item is preserved in
the formatter output. Only `tag:yaml.org,2002:*` core schema tags are stripped;
user tags like `!custom` carry application-defined meaning and must be emitted.

## Test-Document

```yaml
- !custom
- plain
```

## Expected-Document

```yaml
- !custom
- plain
```
