---
test-name: quoted-bool-cursor-at-closing-quote-applies
category: quoted-bool
cursor: 0:14
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
