---
test-name: flow-mapping-bracket-spacing-false-long-breaks
category: flow-style
settings:
  print_width: 20
  bracket_spacing: false
---

# Test: Long Flow Mapping Without Bracket Spacing Closes at the Item Column

With `bracket_spacing: false` a broken flow mapping has no padding inside the
braces, and the closing brace sits at the column of the entries, as it does with
the default `bracket_spacing: true`.

## Test-Document

```yaml
meta: {alpha: one, bravo: two, charlie: three}
```

## Expected-Document

```yaml
meta: {
  alpha: one,
  bravo: two,
  charlie: three
  }
```
