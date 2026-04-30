---
test-name: rename-anchor-no-aliases
category: rename
cursor: 0:5
new-name: orphan
applies-rename: true
---

# Test: Rename anchor with no aliases (orphan anchor)

## Test-Document

```yaml
key: &lonely value
```

## Expected-Document

```yaml
key: &orphan value
```
