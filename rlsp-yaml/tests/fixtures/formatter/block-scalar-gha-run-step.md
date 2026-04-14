---
test-name: block-scalar-gha-run-step
category: block-scalar
---

# Test: GitHub Actions run Step Block Scalar

A `run: |` block scalar nested inside a sequence-item mapping. Content lines
are indented relative to the `run:` key, which is itself inside a sequence
item at one level of indentation.

## Test-Document

```yaml
steps:
  - name: Build
    run: |
      cargo build
      cargo test
```

## Expected-Document

```yaml
steps:
  - name: Build
    run: |
      cargo build
      cargo test
```
