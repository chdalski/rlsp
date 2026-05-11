---
test-name: block-scalar-sequence-item-cursor-on-different-line-omits
category: block-scalar
cursor: 0:0
omits-action: block scalar
---

# Test: No block-scalar action when cursor is not on the sequence item line

## Test-Document

```yaml
- short
- "this is a very long sequence item value that exceeds forty characters"
```
