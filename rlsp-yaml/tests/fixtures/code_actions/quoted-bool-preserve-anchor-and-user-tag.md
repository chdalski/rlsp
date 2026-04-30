---
test-name: quoted-bool-preserve-anchor-and-user-tag
category: quoted-bool
cursor: 0:19
applies-action: Convert quoted
---

# Test: Anchor and user tag on the quoted bool scalar are each preserved once after converting to plain

## Test-Document

```yaml
enabled: &a !mytag "true"
```

## Expected-Document

```yaml
enabled: &a !mytag true
```
