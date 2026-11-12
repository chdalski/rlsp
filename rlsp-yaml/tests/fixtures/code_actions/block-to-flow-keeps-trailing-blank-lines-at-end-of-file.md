---
test-name: block-to-flow-keeps-trailing-blank-lines-at-end-of-file
category: block-to-flow
cursor: 0:0
applies-action: Convert block to flow style
---

# Test: The blank lines at the end of the file stay. The fixture ends with two empty lines before the closing fence; the harness removes only the last line break from both documents.

## Test-Document

```yaml
a:
  b: 1


```

## Expected-Document

```yaml
a: { b: 1 }


```
