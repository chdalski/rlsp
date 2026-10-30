---
test-name: block-scalar-multiline-plain-keeps-following-line
category: block-scalar
cursor: 0:0
applies-action: literal
---

# Test: Convert a multi-line plain scalar without swallowing the next line

The edit ends at the last content byte of the scalar, so the line that
follows keeps its own line.

## Test-Document

```yaml
description: aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa
  bbbb
other: 1
```

## Expected-Document

```yaml
description: |
  aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa bbbb
other: 1
```
