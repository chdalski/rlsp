---
test-name: rename-rejects-name-with-exclamation
category: rename
cursor: 0:5
new-name: name!tag
omits-rename: true
---

# Test: Rename rejects new name containing an exclamation mark

## Test-Document

```yaml
key: &anchor value
```
