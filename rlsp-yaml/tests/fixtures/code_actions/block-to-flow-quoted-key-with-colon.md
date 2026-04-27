---
test-name: block-to-flow-quoted-key-with-colon
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Preserve quoted key with colon when converting block mapping to flow

## Test-Document

```yaml
labels:
  "foo:bar": value
  safe: ok
```

## Expected-Document

```yaml
labels: { foo:bar: value, safe: ok }
```
