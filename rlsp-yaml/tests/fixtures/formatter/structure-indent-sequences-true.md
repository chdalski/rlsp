---
test-name: structure-indent-sequences-true
category: structure
settings:
  format_indent_sequences: true
---

# Test: Indented Block Sequence (format_indent_sequences: true, explicit)

When `format_indent_sequences: true` (the default), a block sequence that is a
mapping value is indented one level under its key. This fixture sets the value
explicitly to document the behavior and serve as a regression check. Input that
uses indentless style is normalized to indented output.

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
