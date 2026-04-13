---
test-name: ecosystem-k8s-deployment-flow-command
category: ecosystem
---

# Test: K8s Deployment Flow Sequence Command Items Preserved

After formatting, the flow sequence under `command:` must be preserved as a
flow sequence inline (not converted to block items). The formatter respects
`CollectionStyle::Flow`, so `command: [...]` stays on one line.

Ref: Kubernetes API — Deployment v1 containers/command

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

## Expected-Document

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
          command: [python, "-m", http.server, "5000"]
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
