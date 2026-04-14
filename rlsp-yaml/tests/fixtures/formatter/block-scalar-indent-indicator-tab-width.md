---
test-name: block-scalar-indent-indicator-tab-width
category: block-scalar
settings:
  tab_width: 4
---

# Test: Explicit Indentation Indicator Matches tab_width

When a block scalar requires an explicit indentation indicator (because the
decoded value's first non-empty line begins with a space), the emitted digit
equals `tab_width`. With `tab_width: 4` the formatter emits `|4` instead of
`|2`, so the YAML parser uses the correct base indentation for the scalar body.

The input uses `|1` which sets content_indent = 1. The content line `  explicit`
(2 spaces) contributes 1 extra space to the decoded value `" explicit\n"`. With
`tab_width: 4`, the formatter emits `|4` and indents content by 4 + 1 = 5 spaces.

## Test-Document

```yaml
- |1
  explicit
```

## Expected-Document

```yaml
- |4
     explicit
```
