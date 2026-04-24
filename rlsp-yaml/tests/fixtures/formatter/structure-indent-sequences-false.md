---
test-name: structure-indent-sequences-false
category: structure
settings:
  format_indent_sequences: false
---

# Test: Indentless Block Sequence (format_indent_sequences: false)

When `format_indent_sequences: false`, a block sequence that is a mapping value
starts at the same column as its key rather than being indented one level under it.
Input that uses indented style is normalized to indentless output.

## Test-Document

```yaml
items:
  - one
  - two
```

## Expected-Document

```yaml
items:
- one
- two
```
