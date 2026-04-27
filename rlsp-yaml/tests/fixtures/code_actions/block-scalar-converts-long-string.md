---
test-name: block-scalar-converts-long-string
category: block-scalar
cursor: 0:0
applies-action: block scalar
---

# Test: Convert long double-quoted string to block scalar

## Test-Document

```yaml
description: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
```

## Expected-Document

```yaml
description: |
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```
