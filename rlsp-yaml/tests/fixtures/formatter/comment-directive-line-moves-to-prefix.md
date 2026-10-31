---
test-name: comment-directive-line-moves-to-prefix
category: comments
---

# Test: Comment On A Directive Line Moves Above The Document Start

The formatter drops the `%YAML` directive. A comment on its line stays as a comment before `---`.

## Test-Document

```yaml
%YAML 1.2 # c
---
a: 1
```

## Expected-Document

```yaml
# c
---
a: 1
```
