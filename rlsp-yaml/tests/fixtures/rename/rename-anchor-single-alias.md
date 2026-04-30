---
test-name: rename-anchor-single-alias
category: rename
cursor: 0:10
new-name: new
applies-rename: true
---

# Test: Rename anchor with single alias

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
