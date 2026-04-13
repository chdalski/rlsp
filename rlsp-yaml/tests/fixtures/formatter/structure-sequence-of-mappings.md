---
test-name: structure-sequence-of-mappings
category: structure
---

# Test: Sequence of Mappings (K8s Pattern)

Continuation keys in a sequence item mapping are indented under the first key,
not at the `- ` column level. This is the common Kubernetes resource list pattern.

## Test-Document

```yaml
users:
  - name: Alice
    age: 30
  - name: Bob
    age: 25
```

## Expected-Document

```yaml
users:
  - name: Alice
    age: 30
  - name: Bob
    age: 25
```
