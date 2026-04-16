---
test-name: preserve-quotes-flow-sequence-kubernetes
category: quoting
settings:
  preserve_quotes: true
---

# Test: Kubernetes Command Array Stays Fully Quoted

The motivating case from the user report: a Kubernetes-style command array where
every element is double-quoted. With `preserve_quotes: true`, safe-plain values
like `"python"` and `"http.server"` stay quoted; values that `needs_quoting` flags
like `"-m"` and `"5000"` also stay quoted (as before). The entire array is unchanged.

## Test-Document

```yaml
command: ["python", "-m", "http.server", "5000"]
```

## Expected-Document

```yaml
command: ["python", "-m", "http.server", "5000"]
```
