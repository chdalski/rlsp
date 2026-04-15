---
test-name: quoting-double-quoted-tab-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar Starting With Tab Stays Quoted

A double-quoted scalar whose decoded value starts with a tab character must stay
double-quoted. A raw tab at the start of a plain scalar is not valid YAML, so
the formatter must preserve the double-quoted form with `\t` escape.

Ref: yaml-test-suite CPZ3[0]

## Test-Document

```yaml
---
tab: "\tstring"
```

## Expected-Document

```yaml
---
tab: "\tstring"
```
