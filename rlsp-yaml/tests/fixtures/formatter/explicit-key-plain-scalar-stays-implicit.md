---
test-name: explicit-key-plain-scalar-stays-implicit
category: explicit-key
---

# Test: Plain Scalar Key Stays Implicit

A plain scalar key must not trigger explicit key form — it renders as `key: value`.

## Test-Document

```yaml
name: Alice
```

## Expected-Document

```yaml
name: Alice
```
