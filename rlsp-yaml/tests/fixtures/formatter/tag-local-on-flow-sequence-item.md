---
test-name: tag-local-on-flow-sequence-item
category: tag
---

# Test: Local Tag on Flow Sequence Item

A local tag (`!local`) on a scalar item inside a flow sequence is preserved in
the formatter output. Local tags (primary-handle tags) are not core schema tags
and must be emitted as-is.

## Test-Document

```yaml
items: [!local a, b]
```

## Expected-Document

```yaml
items: [!local a, b]
```
