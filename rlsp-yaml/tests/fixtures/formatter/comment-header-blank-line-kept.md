---
test-name: comment-header-blank-line-kept
category: blank-lines
idempotent: true
---

# Test: Blank Line After A Header Comment Is Kept

The blank line between a leading comment and the first key stays, as blank lines do everywhere else.

## Test-Document

```yaml
# h

a: 1
```
