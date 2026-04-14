---
test-name: quoting-double-quoted-c0-hex-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Generic C0 Control Stays Quoted

A double-quoted scalar whose decoded value contains a C0 control character
without a named YAML escape (e.g. U+0001 SOH) must stay double-quoted with a
`\xNN` hex escape.

## Test-Document

```yaml
key: "\x01value"
```

## Expected-Document

```yaml
key: "\x01value"
```
