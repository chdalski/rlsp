---
test-name: block-to-flow-multibyte-keeps-comment
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: Characters outside ASCII in the key, the value and the comment do not shift the edit.

## Test-Document

```yaml
ключ:
  b: 日本語 # コメント
d: 1
```

## Expected-Document

```yaml
ключ: { b: 日本語 } # コメント
d: 1
```
