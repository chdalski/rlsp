---
test-name: rename-rejects-name-with-asterisk
category: rename
cursor: 0:5
new-name: name*other
omits-rename: true
---

# Test: Rename rejects new name containing an asterisk

## Test-Document

```yaml
key: &anchor value
```
