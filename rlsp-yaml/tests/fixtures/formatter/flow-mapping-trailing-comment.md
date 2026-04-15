---
test-name: flow-mapping-trailing-comment
category: flow-style
---

# Test: Trailing Comment Preserved With Flow Mapping

A trailing comment on a line containing a flow mapping is preserved. The
flow mapping value and comment both appear on the same line. The formatter
applies default flow-brace spacing (`flow_brace_spacing: true`), so the
output uses `{ a: 1 }` form.

## Test-Document

```yaml
x: {a: 1}  # inline comment
```

## Expected-Document

```yaml
x: { a: 1 }  # inline comment
```
