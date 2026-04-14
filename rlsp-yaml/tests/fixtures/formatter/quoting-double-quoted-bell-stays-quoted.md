---
test-name: quoting-double-quoted-bell-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Bell Stays Quoted

A double-quoted scalar whose decoded value contains a bell character (U+0007)
must stay double-quoted with the `\a` escape preserved.

## Test-Document

```yaml
key: "\a"
```

## Expected-Document

```yaml
key: "\a"
```
