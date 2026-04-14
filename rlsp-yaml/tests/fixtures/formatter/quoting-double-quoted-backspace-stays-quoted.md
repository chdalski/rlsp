---
test-name: quoting-double-quoted-backspace-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Backspace Stays Quoted

A double-quoted scalar whose decoded value contains a backspace character
(U+0008) must stay double-quoted with the `\b` escape sequence preserved.
Backspace is a C0 control character that cannot appear in a plain scalar.

Ref: yaml-test-suite G4RS[0]

## Test-Document

```yaml
control: "\b1998\t1999\t2000\n"
```

## Expected-Document

```yaml
control: "\b1998\t1999\t2000\n"
```
