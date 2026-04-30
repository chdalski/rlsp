---
test-name: rename-rejects-cursor-beyond-line-length
category: rename
cursor: 0:100
new-name: anything
omits-rename: true
---

# Test: Rename returns None when cursor character is beyond line end

## Test-Document

```yaml
key: &anchor value
```
