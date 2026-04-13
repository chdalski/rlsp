---
test-name: flow-mapping-string-values
category: flow-style
---

# Test: Flow Mapping With String Values

A flow mapping with plain string values is preserved as flow style with
bracket spacing applied.

## Test-Document

```yaml
labels: {app: web, env: prod}
```

## Expected-Document

```yaml
labels: { app: web, env: prod }
```
