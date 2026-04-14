---
test-name: anchor-root-block-sequence
category: anchor
---

# Test: Anchor on a Root-Level Block Sequence

An anchor on a root-level block sequence is emitted on its own line, with the
sequence items on subsequent lines. Inline placement (`&anchor - item`) is
invalid YAML — the block sequence indicator cannot appear after a node property
on the same line.

## Test-Document

```yaml
&sequence
- a
```

## Expected-Document

```yaml
&sequence
- a
```
