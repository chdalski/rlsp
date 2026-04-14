---
test-name: explicit-key-single-quoted-stays-implicit
category: explicit-key
---

# Test: Single-Quoted Scalar Key Stays Implicit

A single-quoted scalar key must not trigger explicit key form.

## Test-Document

```yaml
'key': value
```

## Expected-Document

```yaml
key: value
```
