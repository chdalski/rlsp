---
test-name: quoting-flow-sequence-quoted-items-stripped
category: quoting
---

# Test: Safe Quoted Strings in Flow Sequence Are Stripped

In a flow sequence, quoted strings that are safe as plain scalars have their
quotes removed. Strings that need quoting (e.g., starting with `-`) keep their
quotes.

`python` and `http.server` are plain-safe, so their quotes are stripped.
`-m` starts with `-`, which needs quoting to distinguish it from a block
sequence item indicator.

## Test-Document

```yaml
args: ["python", "-m", "http.server"]
```

## Expected-Document

```yaml
args: [python, "-m", http.server]
```
