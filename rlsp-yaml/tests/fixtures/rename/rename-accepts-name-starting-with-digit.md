---
test-name: rename-accepts-name-starting-with-digit
category: rename
cursor: 0:5
new-name: 123abc
applies-rename: true
---

# Test: Rename accepts anchor name starting with a digit

## Test-Document

```yaml
key: &anchor value
```

## Expected-Document

```yaml
key: &123abc value
```
