---
test-name: block-to-flow-empty-last-value
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: An empty last value ends the edit right after its colon; the comment stays.

The double space in `c:  }` is the known trailing space the formatter prints after the colon of an empty value (the same quirk as the `a: ` entry in `.ai/backlog.md`, here in its flow-mapping form, which no backlog entry names). The fixture pins the range of the edit and the text around it; if the quirk is fixed, update the expected document.

## Test-Document

```yaml
a:
  b: 1
  c:
# n
d: 1
```

## Expected-Document

```yaml
a: { b: 1, c:  }
# n
d: 1
```
