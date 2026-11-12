---
test-name: block-to-flow-disabled-by-comment-in-an-indentless-sequence
category: block-to-flow
cursor: 0:0
disables-action: Convert block to flow style
disabled-reason: Flow style would lose the comments in this collection
---

# Test: A comment behind an item of a sequence that is indented like its key is inside the converted range.

## Test-Document

```yaml
a:
- x # c
- y
b: 1
```
