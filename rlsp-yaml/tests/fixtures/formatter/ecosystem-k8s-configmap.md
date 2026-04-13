---
test-name: ecosystem-k8s-configmap
category: ecosystem
idempotent: true
---

# Test: K8s ConfigMap Round-Trip

A Kubernetes ConfigMap with simple key/value data. Verifies that the formatter
is idempotent: format(format(input)) == format(input).

Ref: Kubernetes API — ConfigMap v1

## Test-Document

```yaml
apiVersion: v1
kind: ConfigMap
metadata:
  name: app-config
  namespace: default
data:
  server_port: "8080"
  server_host: 0.0.0.0
  app_name: MyApp
  debug: "false"
```
