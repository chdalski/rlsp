---
test-name: quoting-double-quoted-mixed-escapes-stays-quoted
category: quoting
---

# Test: Double-Quoted Scalar With Multiple Escape Types Stays Quoted

A double-quoted scalar whose decoded value contains multiple control characters
simultaneously must stay double-quoted with all escapes preserved.

Ref: yaml-test-suite G4RS[0] (control field)

## Test-Document

```yaml
control: "\b1998\t1999\t2000\n"
```

## Expected-Document

```yaml
control: "\b1998\t1999\t2000\n"
```
