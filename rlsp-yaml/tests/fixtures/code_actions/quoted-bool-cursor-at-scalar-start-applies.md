---
test-name: quoted-bool-cursor-at-scalar-start-applies
category: quoted-bool
cursor: 0:9
applies-action: Convert quoted
---

# Test: Bool conversion offered when cursor is at scalar start column

## Test-Document

```yaml
enabled: "true"
```

## Expected-Document

```yaml
enabled: true
```
