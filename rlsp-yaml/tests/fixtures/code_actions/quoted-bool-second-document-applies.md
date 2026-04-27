---
test-name: quoted-bool-second-document-applies
category: quoted-bool
cursor: 2:7
applies-action: Convert quoted
---

# Test: Bool conversion offered for quoted bool in second document

## Test-Document

```yaml
key: value
---
flag: "true"
```

## Expected-Document

```yaml
key: value
---
flag: true
```
