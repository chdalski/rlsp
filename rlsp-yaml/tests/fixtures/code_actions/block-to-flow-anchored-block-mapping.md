---
test-name: block-to-flow-anchored-block-mapping
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Preserve anchor when converting anchored block mapping to flow

## Test-Document

```yaml
defaults: &base
  timeout: 30
  retries: 3
```

## Expected-Document

```yaml
defaults: &base { timeout: 30, retries: 3 }
```
