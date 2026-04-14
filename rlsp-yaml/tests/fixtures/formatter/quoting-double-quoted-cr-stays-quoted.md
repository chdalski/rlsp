---
test-name: quoting-double-quoted-cr-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Carriage Return Stays Quoted

A double-quoted scalar whose decoded value contains a carriage return (U+000D)
must stay double-quoted with the `\r` escape preserved.

## Test-Document

```yaml
key: "foo\rbar"
```

## Expected-Document

```yaml
key: "foo\rbar"
```
