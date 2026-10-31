---
test-name: comment-document-all-four-places-multi-document
category: comments
idempotent: true
---

# Test: Comments In Every Document List Of Several Documents

A comment before `---`, after `---`, after the root, and after the last `...` stays where it is, in each document.

## Test-Document

```yaml
# p
---
# m
a: 1
# e
...
# z
---
# m2
b: 2
# e2
...
# after
```
