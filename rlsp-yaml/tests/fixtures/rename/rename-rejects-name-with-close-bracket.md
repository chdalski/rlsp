---
test-name: rename-rejects-name-with-close-bracket
category: rename
cursor: 0:5
new-name: bad]name
omits-rename: true
---

# Test: Rename rejects new name containing a close bracket

## Test-Document

```yaml
key: &anchor value
```
