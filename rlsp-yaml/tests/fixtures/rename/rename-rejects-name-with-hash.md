---
test-name: rename-rejects-name-with-hash
category: rename
cursor: 0:5
new-name: name#comment
omits-rename: true
---

# Test: Rename rejects new name containing a hash character

## Test-Document

```yaml
key: &anchor value
```
