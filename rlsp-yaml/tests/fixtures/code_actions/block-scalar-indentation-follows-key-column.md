---
test-name: block-scalar-indentation-follows-key-column
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Block scalar body indentation tracks the key column (2-space indented key → 4-space body)

## Test-Document

```yaml
  key: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
```

## Expected-Document

```yaml
  key: |
    aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```
