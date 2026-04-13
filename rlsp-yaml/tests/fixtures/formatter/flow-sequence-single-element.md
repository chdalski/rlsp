---
test-name: flow-sequence-single-element
category: flow-style
---

# Test: Single-Element Flow Sequence Stays Inline

A single-element flow sequence is preserved inline. Strings that require
quoting (e.g., starting with `--`) stay quoted.

## Test-Document

```yaml
args: ["--verbose"]
```

## Expected-Document

```yaml
args: ["--verbose"]
```
