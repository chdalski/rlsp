---
test-name: rename-rejects-cursor-beyond-document-lines
category: rename
cursor: 10:0
new-name: anything
omits-rename: true
---

# Test: Rename returns None when cursor line is beyond document end

## Test-Document

```yaml
key: &anchor value
```
