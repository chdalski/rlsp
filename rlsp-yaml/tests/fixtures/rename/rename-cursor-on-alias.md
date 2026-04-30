---
test-name: rename-cursor-on-alias
category: rename
cursor: 3:7
new-name: new
applies-rename: true
---

# Test: Rename triggered from alias position renames both anchor and alias

## Test-Document

```yaml
defaults: &old
  key: val
production:
  <<: *old
```

## Expected-Document

```yaml
defaults: &new
  key: val
production:
  <<: *new
```
