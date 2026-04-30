---
test-name: rename-accepts-name-with-hyphen
category: rename
cursor: 0:5
new-name: valid-name
applies-rename: true
---

# Test: Rename accepts anchor name containing hyphen

## Test-Document

```yaml
key: &anchor value
```

## Expected-Document

```yaml
key: &valid-name value
```
