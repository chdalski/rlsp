---
test-name: comment-at-document-start
category: comments
---

# Test: Comment at Document Start

A comment at the very beginning of the document is preserved as the first line
of output.

## Test-Document

```yaml
# top comment
key: value
```

## Expected-Document

```yaml
# top comment
key: value
```
