---
test-name: quoted-bool-single-quoted-true-to-unquoted
category: quoted-bool
cursor: 0:9
applies-action: Convert quoted
---

# Test: Convert single-quoted 'true' to unquoted true

## Test-Document

```yaml
active: 'true'
```

## Expected-Document

```yaml
active: true
```
