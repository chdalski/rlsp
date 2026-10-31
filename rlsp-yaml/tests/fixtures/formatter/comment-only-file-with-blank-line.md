---
test-name: comment-only-file-with-blank-line
category: comments
idempotent: true
---

# Test: Comment-Only File Keeps Its Comments And Blank Line

A file that holds nothing but comments loads as no document; its comments and the blank line between them stay.

## Test-Document

```yaml
# a

# b
```
