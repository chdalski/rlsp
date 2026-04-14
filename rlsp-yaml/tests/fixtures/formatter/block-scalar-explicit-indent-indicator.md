---
test-name: block-scalar-explicit-indent-indicator
category: block-scalar
---

# Test: Block Scalar Explicit Indentation Indicator

When the decoded value of a block scalar's first non-empty line begins with a
space character, the formatter emits an explicit indentation indicator digit
(e.g. `|2` or `>2`). Without it, the YAML parser would auto-detect a higher
indentation level from the leading space, misinterpreting it as deeper nesting
and producing a different decoded value on re-parse.

This arises when the original YAML used a small explicit indicator (e.g. `|1`)
and the content had one more space than the indicator value — the extra space
survives into the decoded value (e.g. `" explicit\n"`). The formatter must
re-emit the indicator matching `tab_width` (default 2) so the round-trip value
is preserved.

## Test-Document

```yaml
- |1
  explicit
- >1
  folded
```

## Expected-Document

```yaml
- |2
   explicit
- >2
   folded
```
