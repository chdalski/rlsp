---
test-name: comment-line-after-plain-mapping-value-idempotent
category: comments
idempotent: true
---

# Test: Indented Comment Line After Plain Mapping Value (Idempotent)

The output of the matching fixture stays unchanged when formatted again.

## Test-Document

```yaml
a: v
# d
b: 1
```
