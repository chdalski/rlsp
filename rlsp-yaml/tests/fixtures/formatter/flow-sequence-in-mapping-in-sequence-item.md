---
test-name: flow-sequence-in-mapping-in-sequence-item
category: flow-style
---

# Test: Flow Sequence Inside Mapping Inside Sequence Item

A flow sequence nested three levels deep (inside a mapping key that is a
sequence item) retains its flow style.

Ref: K8s container command pattern — flow sequences for command arguments

## Test-Document

```yaml
spec:
  containers:
    - name: test
      command: ["python", "-m", "http.server", "5000"]
```

## Expected-Document

```yaml
spec:
  containers:
    - name: test
      command: [python, "-m", http.server, "5000"]
```
