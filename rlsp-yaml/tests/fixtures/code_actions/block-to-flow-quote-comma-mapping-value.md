---
test-name: block-to-flow-quote-comma-mapping-value
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Quote mapping value containing comma when converting block mapping to flow

## Test-Document

```yaml
info:
  tags: foo, bar
  name: safe
```

## Expected-Document

```yaml
info: { tags: "foo, bar", name: safe }
```
