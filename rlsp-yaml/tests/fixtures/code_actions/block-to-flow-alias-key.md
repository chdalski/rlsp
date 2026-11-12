---
test-name: block-to-flow-alias-key
category: block-to-flow
cursor: 1:0
applies-action: Convert block to flow style
---

# Test: An alias as key keeps its space before the colon.

## Test-Document

```yaml
x: &a k
*a :
  b: 1
d: 1
```

## Expected-Document

```yaml
x: &a k
*a : { b: 1 }
d: 1
```
