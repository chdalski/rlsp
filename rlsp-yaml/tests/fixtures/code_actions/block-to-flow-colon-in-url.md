---
test-name: block-to-flow-colon-in-url
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Produce valid YAML for mapping value containing colon (URL)

## Test-Document

```yaml
endpoint:
  url: http://example.com
  method: GET
```

## Expected-Document

```yaml
endpoint: { url: http://example.com, method: GET }
```
