---
test-name: block-scalar-below-char-threshold-omits
category: block-scalar
cursor: 0:0
omits-action: block scalar
---

# Test: No block-scalar action when value is below the 40-char threshold

## Test-Document

```yaml
key: "short string under forty"
```
