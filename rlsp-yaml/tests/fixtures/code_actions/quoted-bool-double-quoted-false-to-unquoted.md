---
test-name: quoted-bool-double-quoted-false-to-unquoted
category: quoted-bool
cursor: 0:7
applies-action: Convert quoted
---

# Test: Convert double-quoted "false" to unquoted false

## Test-Document

```yaml
flag: "false"
```

## Expected-Document

```yaml
flag: false
```
