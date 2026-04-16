---
test-name: preserve-quotes-needs-quoting-double-kept
category: quoting
settings:
  preserve_quotes: true
---

# Test: Needs-Quoting Double-Quoted Scalar Stays Double-Quoted

A double-quoted scalar that `needs_quoting` flags (here: `5000` is numeric-ambiguous)
is preserved as double-quoted. The `needs_quoting=true` branch is untouched — the
already-quoted-stays-original-style behavior that existed before this task is unchanged.

## Test-Document

```yaml
key: "5000"
```

## Expected-Document

```yaml
key: "5000"
```
