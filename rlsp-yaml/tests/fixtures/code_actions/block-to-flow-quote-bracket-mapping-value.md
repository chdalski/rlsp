---
test-name: block-to-flow-quote-bracket-mapping-value
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Quote mapping value containing bracket when converting block mapping to flow

## Test-Document

```yaml
filter:
  pattern: a[0]
  name: safe
```

## Expected-Document

```yaml
filter: { pattern: "a[0]", name: safe }
```
