---
test-name: rename-accepts-name-with-underscore
category: rename
cursor: 0:5
new-name: valid_name
applies-rename: true
---

# Test: Rename accepts anchor name containing underscore

## Test-Document

```yaml
key: &anchor value
```

## Expected-Document

```yaml
key: &valid_name value
```
