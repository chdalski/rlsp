---
test-name: block-scalar-char-count-not-byte-count-omits
category: block-scalar
cursor: 0:0
omits-action: block scalar
---

# Test: No block-scalar action for 39 multibyte chars even though byte length exceeds threshold

## Test-Document

```yaml
key: "ééééééééééééééééééééééééééééééééééééééé"
```
