---
test-name: block-to-flow-respects-configured-print-width
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
format-options:
  print_width: 120
---

# Test: Respect configured print_width when wrapping converted flow output

The single-line flow form of this sequence is ~92 characters. Under the default
`formatPrintWidth` of 80 it would wrap across multiple lines. With
`formatPrintWidth: 120` (set via `format-options:` frontmatter), it fits on a
single line.

## Test-Document

```yaml
items:
  - alpha_item_one
  - bravo_item_two
  - charlie_item_three
  - delta_item_four
  - echo_item_five
```

## Expected-Document

```yaml
items: [alpha_item_one, bravo_item_two, charlie_item_three, delta_item_four, echo_item_five]
```
