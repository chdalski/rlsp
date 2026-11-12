---
test-name: block-to-flow-explicit-key-comment-before-colon
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: A comment after the explicit key lies outside the converted range.

## Test-Document

```yaml
? a # c
:
  b: 1
```

## Expected-Document

```yaml
? a # c
: { b: 1 }
```
