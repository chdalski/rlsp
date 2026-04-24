---
test-name: structure-indent-sequences-false-nested
category: structure
settings:
  format_indent_sequences: false
---

# Test: Nested Sequences Remain Correct (format_indent_sequences: false)

When `format_indent_sequences: false`, the indentless style applies at every
nesting level: block sequences that are mapping values become indentless wherever
they appear in the tree. Mappings inside sequence items keep their own indentation
under the `- ` marker, and sub-sequences inside those mappings are also indentless
relative to their own keys.

## Test-Document

```yaml
outer:
  - name: Alice
    tags:
      - admin
      - user
  - name: Bob
    tags:
      - guest
```

## Expected-Document

```yaml
outer:
- name: Alice
  tags:
  - admin
  - user
- name: Bob
  tags:
  - guest
```
