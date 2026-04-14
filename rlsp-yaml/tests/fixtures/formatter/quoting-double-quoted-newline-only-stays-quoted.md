---
test-name: quoting-double-quoted-newline-only-stays-quoted
category: quoting
---

# Test: Double-Quoted Newline-Only Scalars Stay Quoted

A double-quoted scalar whose entire value is one or more newline characters must
stay double-quoted. Emitting the value plain would produce a blank or missing
scalar that parsers would interpret as null.

Ref: yaml-test-suite NAT4[0]

## Test-Document

```yaml
---
f: "\n"
h: "\n\n"
```

## Expected-Document

```yaml
f: "\n"
h: "\n\n"
```
