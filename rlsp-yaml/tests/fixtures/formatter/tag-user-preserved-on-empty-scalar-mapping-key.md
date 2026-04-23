---
test-name: tag-user-preserved-on-empty-scalar-mapping-key
category: tag
---

# Test: User Tag Preserved on Empty Scalar Mapping Key

A user-defined (non-core) tag on an empty scalar mapping key is preserved in
the formatter output. Only `tag:yaml.org,2002:*` core schema tags are stripped;
user tags like `!custom` carry application-defined meaning and must be emitted.

A space before `:` is required so the colon is not parsed as part of the tag URI.

## Test-Document

```yaml
!custom : a
```

## Expected-Document

```yaml
!custom : a
```
