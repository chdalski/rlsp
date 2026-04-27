---
test-name: block-scalar-single-quoted-apostrophe-resolved
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Single-quoted '' escape resolves to a literal apostrophe in the block scalar output

## Test-Document

```yaml
note: 'it''s a long string that should exceed the forty character threshold'
```

## Expected-Document

```yaml
note: |
  it's a long string that should exceed the forty character threshold
```
