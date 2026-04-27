---
test-name: block-scalar-char-count-not-byte-length-omits
category: block-scalar
cursor: 0:0
omits-action: block scalar
---

# Test: No block-scalar action for 8 multibyte chars even though byte length exceeds 8

## Test-Document

```yaml
key: "αβγδεζηθ"
```
