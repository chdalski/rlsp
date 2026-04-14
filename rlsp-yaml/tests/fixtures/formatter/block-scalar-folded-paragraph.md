---
test-name: block-scalar-folded-paragraph
category: block-scalar
---

# Test: Folded Block Scalar Paragraph

A folded block scalar (`>`) folds newlines into spaces when the content has
uniform indentation, so the multi-line input is parsed as a single string by
the parser. The formatter re-emits the single content line indented relative to
the parent key.

## Test-Document

```yaml
description: >
  This is a long
  paragraph that wraps.
```

## Expected-Document

```yaml
description: >
  This is a long paragraph that wraps.
```
