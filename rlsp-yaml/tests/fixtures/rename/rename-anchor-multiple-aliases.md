---
test-name: rename-anchor-multiple-aliases
category: rename
cursor: 0:10
new-name: common
applies-rename: true
---

# Test: Rename anchor with multiple aliases

## Test-Document

```yaml
defaults: &shared
  key: val
dev:
  <<: *shared
prod:
  <<: *shared
```

## Expected-Document

```yaml
defaults: &common
  key: val
dev:
  <<: *common
prod:
  <<: *common
```
