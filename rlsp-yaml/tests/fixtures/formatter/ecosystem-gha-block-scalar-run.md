---
test-name: ecosystem-gha-block-scalar-run
category: ecosystem
---

# Test: GHA Workflow Block Scalar Run Steps

A GitHub Actions workflow with `run: |` block scalar steps containing shell
commands. This is the most common CI pattern — each `run:` step uses a literal
block scalar for multi-line shell scripts. Content lines must be indented
relative to the `run:` key.

Ref: GitHub Actions workflow syntax — `jobs.<job_id>.steps[*].run`

## Test-Document

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: |
          cargo build --release
          echo "Build done"
      - name: Test
        run: |
          cargo test --workspace
          cargo clippy --all-targets
      - name: Check formatting
        run: cargo fmt --check
```

## Expected-Document

```yaml
name: CI

on:
  push:
    branches: [main]
  pull_request:
    branches: [main]

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - name: Build
        run: |
          cargo build --release
          echo "Build done"
      - name: Test
        run: |
          cargo test --workspace
          cargo clippy --all-targets
      - name: Check formatting
        run: cargo fmt --check
```
