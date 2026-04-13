---
test-name: comment-multiple-trailing-at-eof
category: comments
---

# Test: Multiple Trailing Comments at EOF with Blank Line Separator

Multiple comments after the last value, separated by a blank line, are all
preserved and appear in order after the content.

## Test-Document

```yaml
key: value
# first EOF comment

# second EOF comment
```

## Expected-Document

```yaml
key: value
# first EOF comment

# second EOF comment
```
