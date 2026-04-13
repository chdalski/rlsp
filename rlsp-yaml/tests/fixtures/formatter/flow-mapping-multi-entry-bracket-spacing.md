---
test-name: flow-mapping-multi-entry-bracket-spacing
category: flow-style
---

# Test: Multi-Entry Flow Mapping With Bracket Spacing

A multi-entry flow mapping with the default `bracket_spacing: true` adds spaces
inside braces and separates entries with commas.

## Test-Document

```yaml
meta: {a: 1, b: 2}
```

## Expected-Document

```yaml
meta: { a: 1, b: 2 }
```
