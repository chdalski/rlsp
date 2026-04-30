---
test-name: rename-rejects-name-with-ampersand
category: rename
cursor: 0:5
new-name: name&other
omits-rename: true
---

# Test: Rename rejects new name containing an ampersand

## Test-Document

```yaml
key: &anchor value
```
