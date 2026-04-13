---
test-name: ecosystem-k8s-service
category: ecosystem
idempotent: true
---

# Test: K8s Service Round-Trip

A Kubernetes Service with `status: {}`. Verifies the formatter is idempotent
and that the empty flow map is preserved.

Ref: Kubernetes API — Service v1

## Test-Document

```yaml
apiVersion: v1
kind: Service
metadata:
  name: web
  namespace: default
spec:
  selector:
    app: web
  ports:
    - protocol: TCP
      port: 80
      targetPort: 5000
  type: ClusterIP
status: {}
```
