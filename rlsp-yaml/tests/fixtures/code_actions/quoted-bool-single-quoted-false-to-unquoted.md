---
test-name: quoted-bool-single-quoted-false-to-unquoted
category: quoted-bool
cursor: 0:10
applies-action: Convert quoted
---

# Test: Convert single-quoted 'false' to unquoted false

## Test-Document

```yaml
enabled: 'false'
```

## Expected-Document

```yaml
enabled: false
```
