---
test-name: anchor-merge-key
category: anchor
---

# Test: Merge Key Pattern

The YAML merge key (`<<: *name`) is a common pattern for inheriting default
values. The alias reference in the merge key value is preserved, and the anchor
definition on the referenced mapping is preserved.

## Test-Document

```yaml
defaults: &defaults
  retries: 3
  timeout: 30
production:
  <<: *defaults
  timeout: 60
```

## Expected-Document

```yaml
defaults: &defaults
  retries: 3
  timeout: 30
production:
  <<: *defaults
  timeout: 60
```
