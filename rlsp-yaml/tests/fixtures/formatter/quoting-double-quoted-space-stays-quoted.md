---
test-name: quoting-double-quoted-space-stays-quoted
category: quoting
---

# Test: Double-Quoted Single-Space Scalar Stays Quoted

A double-quoted scalar whose entire value is a single space must stay
double-quoted. An unquoted ` ` value would look like an empty value to parsers
and be interpreted as null.

Ref: yaml-test-suite NAT4[0] (double-quoted variant)

## Test-Document

```yaml
d: " "
```

## Expected-Document

```yaml
d: " "
```
