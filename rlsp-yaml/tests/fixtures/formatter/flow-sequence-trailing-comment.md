---
test-name: flow-sequence-trailing-comment
category: flow-style
---

# Test: Trailing Comment Preserved With Flow Sequence

A trailing comment on a line containing a flow sequence is preserved on the
same line as the flow sequence.

## Test-Document

```yaml
items: [a, b]  # my comment
```

## Expected-Document

```yaml
items: [a, b]  # my comment
```
