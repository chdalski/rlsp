---
test-name: ecosystem-gha-on-key-unquoted
category: ecosystem
---

# Test: GHA on: Key Stays Unquoted After Format

The `on:` key in a GitHub Actions workflow must not be quoted to `"on":` by
the formatter. Verifies the formatter preserves `on:` as a plain key.

Ref: GitHub Actions workflow syntax — `on` event trigger

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
