---
test-name: ecosystem-flow-collections-preserved
category: ecosystem
---

# Test: Flow Collections Preserved After Format

A document containing both a flow sequence and a flow mapping. Both must be
preserved (not converted to block style) after formatting.

## Test-Document

```yaml
name: example
tags: [web, backend, rust]
labels: {env: prod, app: myapp}
```

## Expected-Document

```yaml
name: example
tags: [web, backend, rust]
labels: { env: prod, app: myapp }
```
