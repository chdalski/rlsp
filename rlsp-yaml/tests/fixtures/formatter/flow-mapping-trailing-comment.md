---
test-name: flow-mapping-trailing-comment
category: flow-style
---

# Test: Trailing Comment Preserved With Flow Mapping

A trailing comment on a line containing a flow mapping is preserved. The
flow mapping value and comment both appear on the same line.

## Test-Document

```yaml
x: {a: 1}  # inline comment
```

## Expected-Document

```yaml
x: {a: 1}  # inline comment
```
