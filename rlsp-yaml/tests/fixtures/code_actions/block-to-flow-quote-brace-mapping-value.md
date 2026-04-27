---
test-name: block-to-flow-quote-brace-mapping-value
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Quote mapping value containing brace when converting block mapping to flow

## Test-Document

```yaml
template:
  expr: ${VAR}
  name: safe
```

## Expected-Document

```yaml
template: { expr: "${VAR}", name: safe }
```
