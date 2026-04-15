---
test-name: interact-bracket-spacing-print-width
category: interaction
settings:
  bracket_spacing: true
  print_width: 20
---

# Test: bracket_spacing + print_width — Narrow Width Breaks Flow Mapping With Spacing

When `bracket_spacing: true` (the default) and `print_width: 20`, a flow
mapping that does not fit on a single line is broken across lines. The opening
brace retains its trailing space (`{ `) and the closing brace retains its
leading space (` }`), visible at the break points.

This differs from `bracket_spacing: false`, where breaks produce `{` and `}`
with no inner spaces.

## Test-Document

```yaml
meta: {alpha: one, bravo: two}
```

## Expected-Document

```yaml
meta: { 
  alpha: one,
  bravo: two
 }
```
