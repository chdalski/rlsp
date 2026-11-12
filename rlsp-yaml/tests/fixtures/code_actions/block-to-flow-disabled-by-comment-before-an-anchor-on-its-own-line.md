---
test-name: block-to-flow-disabled-by-comment-before-an-anchor-on-its-own-line
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment on the key's line, before an anchor on its own line, is inside the converted range.

## Test-Document

```yaml
a: # c
  &x
  b: 1
```
