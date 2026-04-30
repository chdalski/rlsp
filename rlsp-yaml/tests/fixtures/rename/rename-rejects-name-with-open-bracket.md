---
test-name: rename-rejects-name-with-open-bracket
category: rename
cursor: 0:5
new-name: bad[name
omits-rename: true
---

# Test: Rename rejects new name containing an open bracket

## Test-Document

```yaml
key: &anchor value
```
