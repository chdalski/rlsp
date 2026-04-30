---
test-name: rename-rejects-name-with-close-brace
category: rename
cursor: 0:5
new-name: bad}name
omits-rename: true
---

# Test: Rename rejects new name containing a close brace

## Test-Document

```yaml
key: &anchor value
```
