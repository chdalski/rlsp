---
test-name: ecosystem-gha-workflow-flow-sequences
category: ecosystem
---

# Test: GHA Workflow Flow Sequences Preserved

Flow sequences in a GHA workflow (`branches: [main, develop]`) must be
preserved after formatting — not converted to block sequences.

Ref: GitHub Actions workflow syntax

## Test-Document

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - name: Run tests
        run: cargo test --workspace
      - name: Check format
        run: cargo fmt --check
```

## Expected-Document

```yaml
name: CI

on:
  push:
    branches: [main, develop]
  pull_request:
    branches: [main]

permissions:
  contents: read

env:
  CARGO_TERM_COLOR: always

jobs:
  test:
    name: Test
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v6
      - name: Run tests
        run: cargo test --workspace
      - name: Check format
        run: cargo fmt --check
```
