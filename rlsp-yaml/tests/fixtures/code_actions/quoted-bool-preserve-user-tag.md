---
test-name: quoted-bool-preserve-user-tag
category: quoted-bool
cursor: 0:16
applies-action: Convert quoted
---

# Test: User tag on the quoted bool scalar is preserved once after converting to plain

## Test-Document

```yaml
enabled: !mytag "true"
```

## Expected-Document

```yaml
enabled: !mytag true
```
