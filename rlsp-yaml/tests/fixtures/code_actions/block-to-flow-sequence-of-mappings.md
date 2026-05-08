---
test-name: block-to-flow-sequence-of-mappings
category: block-to-flow
cursor: 0:0
applies-action: block to flow
---

# Test: Convert Kubernetes containers shape (sequence of block mappings) to flow style

## Test-Document

```yaml
containers:
  - name: web
    image: nginx
  - name: db
    image: postgres
```

## Expected-Document

```yaml
containers: [{ name: web, image: nginx }, { name: db, image: postgres }]
```
