---
test-name: block-scalar-literal-blank-lines-kept
category: blank-lines
idempotent: true
---

# Test: Blank Lines Inside A Literal Block Scalar Are Part Of Its Value

Two blank lines inside a literal, two leading ones, and the trailing ones of a keep-chomped literal all stay: they are in the scalar's value.

## Test-Document

```yaml
a: |
  x


  y
b: |


  z
c: |+
  w


d: 1
```
