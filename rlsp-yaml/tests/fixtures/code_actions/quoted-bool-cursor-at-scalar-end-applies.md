---
test-name: quoted-bool-cursor-at-scalar-end-applies
category: quoted-bool
cursor: 0:15
applies-action: Convert quoted
---

# Test: Bool conversion offered when cursor is at the closing-quote column

## Test-Document

```yaml
enabled: "true"
```

## Expected-Document

```yaml
enabled: true
```
