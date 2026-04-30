---
test-name: rename-accepts-name-with-dot
category: rename
cursor: 0:5
new-name: valid.name
applies-rename: true
---

# Test: Rename accepts anchor name containing dot

## Test-Document

```yaml
key: &anchor value
```

## Expected-Document

```yaml
key: &valid.name value
```
