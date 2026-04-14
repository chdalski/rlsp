---
test-name: anchor-scalar-definition
category: anchor
---

# Test: Anchor on a Scalar Value

An anchor definition (`&name`) on a plain scalar value is preserved verbatim in
the formatted output.

## Test-Document

```yaml
key: &val value
```

## Expected-Document

```yaml
key: &val value
```
