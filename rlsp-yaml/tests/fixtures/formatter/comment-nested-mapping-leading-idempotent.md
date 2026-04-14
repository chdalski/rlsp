---
test-name: comment-nested-mapping-leading-idempotent
category: comments
idempotent: true
---

# Test: Nested Mapping Leading Comments Are Idempotent

Formatting a document that already has correctly-placed leading comments between
nested mapping entries produces the same output when formatted a second time.

## Test-Document

```yaml
Lists:
  # Style 1
  list-a:
    - item1
    - item2

  # Style 2
  list-b:
    - item1
    - item2
```
