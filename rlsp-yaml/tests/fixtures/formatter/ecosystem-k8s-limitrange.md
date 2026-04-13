---
test-name: ecosystem-k8s-limitrange
category: ecosystem
idempotent: true
---

# Test: K8s LimitRange Round-Trip

A Kubernetes LimitRange with `cpu` and `memory` under multiple sibling mappings
(`max`, `min`, `default`, `defaultRequest`). Verifies the formatter is idempotent.

Ref: Kubernetes API — LimitRange v1

## Test-Document

```yaml
apiVersion: v1
kind: LimitRange
metadata:
  name: cpu-memory-limits
  namespace: default
spec:
  limits:
    - type: Container
      max:
        cpu: "2"
        memory: 1Gi
      min:
        cpu: 100m
        memory: 128Mi
      default:
        cpu: 500m
        memory: 256Mi
      defaultRequest:
        cpu: 200m
        memory: 128Mi
```
