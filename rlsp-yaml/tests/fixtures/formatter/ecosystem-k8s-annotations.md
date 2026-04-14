---
test-name: ecosystem-k8s-annotations
category: ecosystem
---

# Test: K8s Deployment with Annotations

A Kubernetes Deployment with long annotation values (some quoted), labels with
`app.kubernetes.io/` keys. Tests quoting behavior: quoted strings that don't
need quoting are stripped; strings like `"true"` and `"8080"` (which parse as
YAML boolean/integer without quotes) keep their quotes.

Ref: Kubernetes API — Deployment v1, standard label keys

## Test-Document

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  namespace: production
  annotations:
    kubernetes.io/change-cause: "Deploy v2.3.1 — fix memory leak in request handler"
    app.kubernetes.io/version: 2.3.1
    prometheus.io/scrape: "true"
    prometheus.io/port: "8080"
  labels:
    app.kubernetes.io/name: my-app
    app.kubernetes.io/instance: my-app-production
    app.kubernetes.io/version: 2.3.1
    app.kubernetes.io/component: backend
spec:
  replicas: 3
  selector:
    matchLabels:
      app.kubernetes.io/name: my-app
      app.kubernetes.io/instance: my-app-production
  template:
    metadata:
      labels:
        app.kubernetes.io/name: my-app
        app.kubernetes.io/instance: my-app-production
    spec:
      containers:
        - name: my-app
          image: registry.example.com/my-app:2.3.1
```

## Expected-Document

```yaml
apiVersion: apps/v1
kind: Deployment
metadata:
  name: my-app
  namespace: production
  annotations:
    kubernetes.io/change-cause: Deploy v2.3.1 — fix memory leak in request handler
    app.kubernetes.io/version: 2.3.1
    prometheus.io/scrape: "true"
    prometheus.io/port: "8080"
  labels:
    app.kubernetes.io/name: my-app
    app.kubernetes.io/instance: my-app-production
    app.kubernetes.io/version: 2.3.1
    app.kubernetes.io/component: backend
spec:
  replicas: 3
  selector:
    matchLabels:
      app.kubernetes.io/name: my-app
      app.kubernetes.io/instance: my-app-production
  template:
    metadata:
      labels:
        app.kubernetes.io/name: my-app
        app.kubernetes.io/instance: my-app-production
    spec:
      containers:
        - name: my-app
          image: registry.example.com/my-app:2.3.1
```
