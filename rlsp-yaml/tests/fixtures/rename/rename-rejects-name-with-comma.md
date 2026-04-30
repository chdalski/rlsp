---
test-name: rename-rejects-name-with-comma
category: rename
cursor: 0:5
new-name: bad,name
omits-rename: true
---

# Test: Rename rejects new name containing a comma

## Test-Document

```yaml
key: &anchor value
```
