# Rust Conventions

**Agents**: Developer, Test Engineer, Code Reviewer

## Module Structure

- Use `module_name.rs` files, not `mod.rs` for modules.
- Re-export from the parent module with `pub use`.

## Test Placement

- Write unit tests as inline `#[cfg(test)]` modules in
  the same file as the code they test.
- Use `/tests/` only for integration tests that exercise
  multiple modules together.

## Linting

- Use Clippy as helpfull as possible to write better code:

```bash
cargo clippy --all -- -W clippy::all -W clippy::pedantic -W clippy::restriction -W clippy::nursery -D warnings
```
