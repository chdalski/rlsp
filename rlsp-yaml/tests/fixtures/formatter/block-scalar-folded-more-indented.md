---
test-name: block-scalar-folded-more-indented
category: block-scalar
idempotent: true
---

# Test: Folded Scalar More-Indented Lines

In a folded block scalar, "more-indented" lines (lines whose content begins
with a space, placing them at a greater indentation level than the base) have
their line break preserved without needing an extra blank line in the output.
The formatter accounts for this free line break when computing how many blank
lines to emit between adjacent content segments.

When a more-indented line is adjacent to a base-level line, only the `\n`s
beyond the free one require blank lines in the output.

The explicit indentation indicator digit (`>2`) is required because the first
content line of the decoded value starts with a space (` more indented`).

The input uses `>2` which sets content_indent = 2. The first content line
`   more indented` (3 spaces) contributes 1 extra space to the decoded value.
The formatter preserves this by re-emitting `>2` with the same structure.

## Test-Document

```yaml
a: >2
   more indented
  regular
```

