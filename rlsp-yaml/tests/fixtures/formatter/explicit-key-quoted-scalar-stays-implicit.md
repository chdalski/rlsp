---
test-name: explicit-key-quoted-scalar-stays-implicit
category: explicit-key
---

# Test: Double-Quoted Scalar Key Stays Implicit

A double-quoted scalar key must not trigger explicit key form.

## Test-Document

```yaml
"quoted key": value
```

## Expected-Document

```yaml
quoted key: value
```
