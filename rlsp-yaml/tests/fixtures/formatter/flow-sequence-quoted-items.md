---
test-name: flow-sequence-quoted-items
category: flow-style
---

# Test: Flow Sequence With Quoted Items

Items that require quoting (e.g., strings starting with `-`) stay quoted in a
flow sequence. Safe strings like "python" and "http.server" have their quotes
stripped by the formatter.

## Test-Document

```yaml
cmd: ["python", "-m", "http.server"]
```

## Expected-Document

```yaml
cmd: [python, "-m", http.server]
```
