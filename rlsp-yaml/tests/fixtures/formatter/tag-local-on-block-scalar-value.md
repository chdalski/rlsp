---
test-name: tag-local-on-block-scalar-value
category: tag
---

# Test: Local Tag on Block Scalar Mapping Value

A local tag (`!local`) on a scalar mapping value in block context is preserved in
the formatter output. Local tags (primary-handle tags) are not core schema tags
and must be emitted as-is regardless of scalar content.

## Test-Document

```yaml
kind: !local explicit
```

## Expected-Document

```yaml
kind: !local explicit
```
