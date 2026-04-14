---
test-name: quoting-double-quoted-backslash-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Backslash Stays Quoted

A double-quoted scalar whose decoded value contains a backslash must stay
double-quoted. The backslash is re-escaped as `\\` in the output. A plain
scalar cannot represent a backslash without quoting.

## Test-Document

```yaml
path: "C:\\Users\\foo"
```

## Expected-Document

```yaml
path: "C:\\Users\\foo"
```
