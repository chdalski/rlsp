---
test-name: quoted-bool-preserve-anchor
category: quoted-bool
cursor: 0:19
applies-action: Convert quoted
---

# Test: Anchor on the quoted bool scalar is preserved once after converting to plain

## Test-Document

```yaml
enabled: &myanchor "true"
```

## Expected-Document

```yaml
enabled: &myanchor true
```
