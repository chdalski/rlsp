---
test-name: sequence-mapping-compact-marker-tab-width
category: sequence
settings:
  tab_width: 4
---

# Test: Compact Marker Is As Wide As An Indent Level

A mapping that starts on the line of its `- ` (or an explicit key that starts on
the line of its `? `) continues in the column of its first entry. With
`tab_width: 4` the lines after the first would sit four columns in while the
first entry sits two, which reads as other data, so the marker is padded to the
width of one level: `-   a: 1`. The same holds for the `? ` of a key that is a
block collection.

## Test-Document

```yaml
- a: 1
  b: 2
- ? - x
    - y
  : z
```

## Expected-Document

```yaml
-   a: 1
    b: 2
-   ?   - x
        - y
    : z
```
