---
test-name: anchor-alias-reference
category: anchor
---

# Test: Anchor Definition and Alias Reference Round-Trip

A document containing both an anchor definition (`&name`) and an alias reference
(`*name`) is formatted with both preserved. The anchor appears on the value node
that defines it; the alias reference is emitted unchanged.

## Test-Document

```yaml
defaults: &defaults
  timeout: 30
service:
  <<: *defaults
```

## Expected-Document

```yaml
defaults: &defaults
  timeout: 30
service:
  <<: *defaults
```
