---
test-name: interact-bracket-spacing-print-width
category: interaction
settings:
  bracket_spacing: true
  print_width: 20
---

# Test: bracket_spacing + print_width — Narrow Width Breaks Flow Mapping Without Padding

When `bracket_spacing: true` (the default) and `print_width: 20`, a flow
mapping that does not fit on a single line is broken across lines. The padding
inside the braces belongs to the one-line form only: the broken form has no
trailing space after `{`, and the closing brace sits at the column of the
entries.

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
