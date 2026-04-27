---
test-name: quoted-bool-unicode-escaped-true-applies
category: quoted-bool
cursor: 0:8
applies-action: Convert quoted
---

# Test: Bool conversion for unicode-escaped true decodes to true and produces plain true

## Test-Document

```yaml
flag: "\u0074rue"
```

## Expected-Document

```yaml
flag: true
```

