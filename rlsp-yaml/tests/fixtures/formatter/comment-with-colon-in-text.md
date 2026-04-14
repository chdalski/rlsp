---
test-name: comment-with-colon-in-text
category: comments
---

# Test: Inline Comment Containing ': ' Does Not Break Output

An inline comment whose text includes a colon-space sequence (e.g. `# another : bug`)
is preserved verbatim. The colon inside the comment must not confuse the formatter
or downstream YAML parsers.

## Test-Document

```yaml
Lists:
  # Style 1
  list-a:
    - item1 # another : bug
    - item2

  # Style 2
  list-b:
  - item1
  - item2
```

## Expected-Document

```yaml
Lists:
  # Style 1
  list-a:
    - item1  # another : bug
    - item2

  # Style 2
  list-b:
    - item1
    - item2
```
