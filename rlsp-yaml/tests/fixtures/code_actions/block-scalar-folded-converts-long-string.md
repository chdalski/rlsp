---
test-name: block-scalar-folded-converts-long-string
category: block-scalar
cursor: 0:0
applies-action: folded
---

# Test: Convert long double-quoted string to folded block scalar

## Test-Document

```yaml
description: "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa"
```

## Expected-Document

```yaml
description: >
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
```
