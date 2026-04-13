---
test-name: ecosystem-k8s-deployment
category: ecosystem
idempotent: true
---

# Test: K8s Deployment Round-Trip

A Kubernetes Deployment with a flow sequence in `containers/command` and
`status: {}`. Verifies the formatter is idempotent and preserves flow style.

Ref: Kubernetes API — Deployment v1

## Test-Document

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: web
  namespace: default
spec:
  replicas: 1
  selector:
    matchLabels:
      app: web
  template:
    metadata:
      labels:
        app: web
    spec:
      containers:
        - name: web
          image: python:3.11
          command: ["python", "-m", "http.server", "5000"]
          ports:
            - containerPort: 5000
          resources:
            limits:
              cpu: 500m
              memory: 256Mi
            requests:
              cpu: 100m
              memory: 128Mi
status: {}
```
