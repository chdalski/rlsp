---
test-name: quoting-double-quoted-vtab-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Vertical Tab Stays Quoted

A double-quoted scalar whose decoded value contains a vertical tab (U+000B)
must stay double-quoted with the `\v` escape preserved.

## Test-Document

```yaml
key: "\v"
```

## Expected-Document

```yaml
key: "\v"
```
