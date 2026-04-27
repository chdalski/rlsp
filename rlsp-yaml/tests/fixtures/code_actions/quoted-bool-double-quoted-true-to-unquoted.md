---
test-name: quoted-bool-double-quoted-true-to-unquoted
category: quoted-bool
cursor: 0:10
applies-action: Convert quoted
---

# Test: Convert double-quoted "true" to unquoted true

## Test-Document

```yaml
enabled: "true"
```

## Expected-Document

```yaml
enabled: true
```
