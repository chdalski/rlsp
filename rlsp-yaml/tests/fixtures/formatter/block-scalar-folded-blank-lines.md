---
test-name: block-scalar-folded-blank-lines
category: block-scalar
---

# Test: Folded Scalar Blank Line Preservation

In a folded block scalar, adjacent base-level content lines are folded to a
single space on parse. To preserve multiple `\n` characters in the decoded
value, the formatter emits explicit blank lines between content segments:

- Two base-level lines separated by one `\n` in the value: no blank line
  needed (the fold produces exactly one space, but the formatter must still
  emit them as separate lines with one blank between them so the fold
  re-produces the original `\n`).
- Two base-level lines separated by two `\n`s: one blank line between them.
- N `\n`s between two base-level lines: N-1 blank lines.

## Test-Document

```yaml
key: >
  ab cd

  ef


  gh
```

## Expected-Document

```yaml
key: >
  ab cd

  ef


  gh
```
