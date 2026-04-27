---
test-name: quoted-bool-sequence-item-applies
category: quoted-bool
cursor: 1:5
applies-action: Convert quoted
---

# Test: Bool conversion offered for quoted bool as a block sequence item

## Test-Document

```yaml
items:
  - "true"
  - "false"
```

## Expected-Document

```yaml
items:
  - true
  - "false"
```
