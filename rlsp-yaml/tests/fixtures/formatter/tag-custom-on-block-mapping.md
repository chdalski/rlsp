---
test-name: tag-custom-on-block-mapping
category: tag
---

# Test: Custom Tag on a Block Mapping Value

A custom (local) tag on a block mapping value is emitted on the key line,
after the colon separator and before the newline that opens the indented block.
This mirrors the anchor placement for block collections.

## Test-Document

```yaml
key: !custom
  a: 1
```

## Expected-Document

```yaml
key: !custom
  a: 1
```
