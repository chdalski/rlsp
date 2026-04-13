---
test-name: empty-flow-mapping
category: flow-style
---

# Test: Empty Flow Mapping Stays as Braces

An empty flow mapping `{}` is preserved as-is — it does not get converted to
block style.

## Test-Document

```yaml
status: {}
```

## Expected-Document

```yaml
status: {}
```
