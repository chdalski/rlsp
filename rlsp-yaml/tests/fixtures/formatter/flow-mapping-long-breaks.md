---
test-name: flow-mapping-long-breaks
category: flow-style
settings:
  print_width: 20
---

# Test: Long Flow Mapping Breaks Across Lines

When a flow mapping is too wide for the `print_width`, the Wadler-Lindig
pretty-printer breaks it across lines with indented entries.

## Test-Document

```yaml
meta: {alpha: one, bravo: two, charlie: three, delta: four}
```

## Expected-Document

```yaml
meta: { 
  alpha: one,
  bravo: two,
  charlie: three,
  delta: four
 }
```
