---
test-name: quoting-double-quoted-newline-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Newline Stays Quoted

A double-quoted scalar whose decoded value contains a newline character must
stay double-quoted. Without quoting, the newline would split the value across
lines, producing a multi-line plain scalar that parsers reject.

Ref: yaml-test-suite 6SLA[0]

## Test-Document

```yaml
msg: "foo\nbar"
```

## Expected-Document

```yaml
msg: "foo\nbar"
```
