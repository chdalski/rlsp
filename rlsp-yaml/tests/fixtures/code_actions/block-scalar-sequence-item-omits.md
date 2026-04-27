---
test-name: block-scalar-sequence-item-omits
category: block-scalar
cursor: 0:0
omits-action: block scalar
---

# Test: No block-scalar action for a long quoted value that is a sequence item (only mapping values qualify)

## Test-Document

```yaml
- "this is a very long sequence item value that exceeds forty characters"
```
