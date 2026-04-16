---
test-name: preserve-quotes-needs-quoting-single-kept
category: quoting
settings:
  preserve_quotes: true
---

# Test: Needs-Quoting Single-Quoted Scalar Stays Single-Quoted

A single-quoted scalar that `needs_quoting` flags (here: `-m` has a leading dash
that is a reserved character) is preserved as single-quoted. Regression guard for
the `needs_quoting=true` branch.

## Test-Document

```yaml
key: '-m'
```

## Expected-Document

```yaml
key: '-m'
```
